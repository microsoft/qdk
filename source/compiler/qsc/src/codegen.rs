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
    use qsc_fir::ty::{FunctorSet, FunctorSetValue, GenericArg, ParamId, Ty};

    use qsc_data_structures::{
        error::WithSource, functors::FunctorApp, language_features::LanguageFeatures,
        source::SourceMap, target::TargetCapabilityFlags,
    };
    use qsc_frontend::compile::{Dependencies, PackageStore};
    use qsc_partial_eval::{PartialEvalConfig, ProgramEntry};
    use qsc_passes::{PackageType, PassContext, run_rca_for_callable};
    use rustc_hash::{FxHashMap, FxHashSet};

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
            args: Value,
        },
    }

    /// Declaration and runtime-payload types for a reachable global callable,
    /// with its generic parameters. Keep nominal identity for validation while
    /// synthetic lowering and inference use the existing structural form.
    #[derive(Clone)]
    struct CallableValueInfo {
        /// Original signature, including nominal UDT identities.
        declared_ty: qsc_fir::ty::Ty,
        /// Structural signature used to lower erased runtime payloads, retaining
        /// declared generic parameters for inference.
        ty: qsc_fir::ty::Ty,
        /// Declared parameters used to instantiate the callable at its use site.
        generics: Vec<qsc_fir::ty::TypeParameter>,
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

    /// Lowers the HIR package store to FIR, optionally adapting codegen inputs.
    ///
    /// `package_override` replaces only the HIR for `package_id`.
    /// `runtime_callables` selects package-qualified runtime declarations whose
    /// inherited functor bounds must be closed before lowering discards them.
    /// Normalization applies to the selected HIR, including an override, and
    /// copies only packages containing selected nongeneric declarations. Neither
    /// the stored HIR nor the supplied override is mutated.
    ///
    /// Returns the lowered store, the requested package's FIR ID, and an assigner.
    /// The adapted path retains the requested package's assigner; the ordinary
    /// path preserves the standard helper's result, including its last-package
    /// assigner. This performs lowering only, not the subsequent FIR transforms.
    fn lower_to_fir(
        package_store: &PackageStore,
        package_id: qsc_hir::hir::PackageId,
        package_override: Option<&qsc_hir::hir::Package>,
        runtime_callables: Option<&FxHashSet<qsc_fir::fir::StoreItemId>>,
    ) -> (
        qsc_fir::fir::PackageStore,
        qsc_fir::fir::PackageId,
        qsc_fir::assigner::Assigner,
    ) {
        if package_override.is_some() || runtime_callables.is_some() {
            let mut fir_store = qsc_fir::fir::PackageStore::new();
            let mut fir_assigner = qsc_fir::assigner::Assigner::new();

            // Preserve store order so dependencies are available when later
            // packages are lowered and their cross-package references validated.
            for (id, unit) in package_store {
                let fir_id = qsc_lowerer::map_hir_package_to_fir(id);
                // An override affects only the requested package, not its dependencies.
                let hir_package = package_override
                    .filter(|_| id == package_id)
                    .unwrap_or(&unit.package);
                // Runtime declarations can belong to any package. Normalize
                // after selecting the HIR and keep the optional Cow alive until
                // lowering has consumed the borrowed or newly copied package.
                let normalized = runtime_callables.map(|callables| {
                    normalize_runtime_callable_hir(hir_package, fir_id, callables)
                });
                let hir_package = normalized.as_deref().unwrap_or(hir_package);
                // Node and local-variable allocation belong to one package.
                let mut lowerer = qsc_lowerer::Lowerer::new();
                let fir_package = if id == package_id {
                    // Use update-style lowering for the adapted entry package.
                    // It lowers items/statements before the entry and leaves
                    // the entry execution graph in the lowerer for us to install.
                    let mut fir_package = Package {
                        id: fir_id,
                        ..Package::default()
                    };
                    lowerer.lower_and_update_package(&mut fir_package, hir_package);
                    fir_package.entry_exec_graph = lowerer.take_exec_graph();
                    fir_package
                } else {
                    // Complete-package lowering installs its entry graph and
                    // validates references against the FIR store built so far.
                    lowerer.lower_package(hir_package, &fir_store, fir_id)
                };
                if id == package_id {
                    // Retain this package's allocation counters for the caller.
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
            // No codegen-specific HIR adaptation is needed.
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
        //   - No `Ty::Arrow` params / `ExprKind::Closure` (defunctionalization completed).
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
    /// The original declaration type is retained separately for nominal checks.
    /// Generic signatures retain their functor parameters until instantiation.
    fn build_callable_type_map(
        fir_store: &qsc_fir::fir::PackageStore,
        callables: &FxHashSet<qsc_fir::fir::StoreItemId>,
    ) -> rustc_hash::FxHashMap<qsc_fir::fir::StoreItemId, CallableValueInfo> {
        use qsc_fir::fir::{Global, PackageLookup};

        let mut map =
            rustc_hash::FxHashMap::with_capacity_and_hasher(callables.len(), Default::default());
        for id in callables {
            let package = fir_store.get(id.package);
            let Some(Global::Callable(callable_decl)) = package.get_global(id.item) else {
                panic!("callable should exist in lowered package");
            };
            let (_, ty) = callable_expr_span_and_ty(fir_store, *id);
            let structural_ty = resolve_udt_ty(fir_store, &ty);
            let normalized_ty = if callable_decl.generics.is_empty() {
                resolve_functor_params(&structural_ty)
            } else {
                structural_ty
            };
            map.insert(
                *id,
                CallableValueInfo {
                    declared_ty: ty,
                    ty: normalized_ty,
                    generics: callable_decl.generics.clone(),
                },
            );
        }
        map
    }

    /// Checks signature constraints without changing runtime values. Nominal
    /// evidence and erased payload shapes are checked independently; structural
    /// substitutions also supply target type arguments for synthetic lowering.
    /// Structural checking resolves functor variables with their declared lower
    /// bounds. Unknown types and type-class constraints retain their existing checks.
    /// Callback parameters get fresh IDs distinct from the target's declaration IDs;
    /// unresolved IDs never enter the compiler store.
    struct RuntimeArgTypeChecker<'a> {
        /// Read-only declarations used to inspect UDT payload types.
        fir_store: &'a qsc_fir::fir::PackageStore,
        /// Cached declared and structural signatures of the supplied callables.
        callables: &'a FxHashMap<qsc_fir::fir::StoreItemId, CallableValueInfo>,
        /// Whether this pass compares nominal declarations rather than erased payloads.
        nominal: bool,
        /// Substitutions for this pass's fresh, declaration-scoped type parameters.
        bindings: FxHashMap<ParamId, Ty>,
        /// Required capabilities flow to supporting parameters; available
        /// capabilities flow back to their dependents.
        functor_requirements: FxHashMap<ParamId, Vec<ParamId>>,
        functor_dependents: FxHashMap<ParamId, Vec<ParamId>>,
        functor_bounds: FxHashMap<ParamId, FunctorSetValue>,
        functor_limits: FxHashMap<ParamId, FunctorSetValue>,
        /// Opaque values to revisit after declared callbacks constrain their types.
        deferred_values: Vec<(Ty, Value)>,
        /// Next private parameter ID, also used for aggregate element placeholders.
        /// These IDs never enter the compiler's package store.
        next_param: usize,
    }

    impl RuntimeArgTypeChecker<'_> {
        fn new_param_id(&mut self) -> ParamId {
            let param = ParamId::from(self.next_param);
            self.next_param += 1;
            param
        }

        fn new_param(&mut self) -> Ty {
            Ty::Param(self.new_param_id())
        }

        fn fresh_type(&mut self, ty: &Ty, generics: &[qsc_fir::ty::TypeParameter]) -> Ty {
            fn rename(
                checker: &mut RuntimeArgTypeChecker<'_>,
                ty: &mut Ty,
                params: &mut FxHashMap<ParamId, ParamId>,
            ) {
                match ty {
                    Ty::Param(param) => {
                        *param = *params
                            .entry(*param)
                            .or_insert_with(|| checker.new_param_id());
                    }
                    Ty::Array(item) => rename(checker, item, params),
                    Ty::Tuple(items) => {
                        for item in items {
                            rename(checker, item, params);
                        }
                    }
                    Ty::Arrow(arrow) => {
                        rename(checker, &mut arrow.input, params);
                        rename(checker, &mut arrow.output, params);
                        if let FunctorSet::Param(param) = arrow.functors {
                            let fresh = *params
                                .entry(param)
                                .or_insert_with(|| checker.new_param_id());
                            arrow.functors = FunctorSet::Param(fresh);
                        }
                    }
                    Ty::Err | Ty::Infer(_) | Ty::Prim(_) | Ty::Udt(_) => {}
                }
            }
            let mut params = FxHashMap::default();
            for (idx, generic) in generics.iter().enumerate() {
                let fresh = self.new_param_id();
                params.insert(ParamId::from(idx), fresh);
                if let qsc_fir::ty::TypeParameter::Functor(bound) = generic {
                    self.functor_bounds.insert(fresh, *bound);
                }
            }
            let mut ty = ty.clone();
            rename(self, &mut ty, &mut params);
            ty
        }

        fn resolve(&self, ty: &Ty) -> Ty {
            let mut ty = ty;
            while let Ty::Param(param) = ty
                && let Some(bound) = self.bindings.get(param)
            {
                ty = bound;
            }
            ty.clone()
        }

        fn resolve_type(&self, ty: &Ty) -> Ty {
            match self.resolve(ty) {
                Ty::Array(item) => Ty::Array(Box::new(self.resolve_type(&item))),
                Ty::Tuple(items) => {
                    Ty::Tuple(items.iter().map(|item| self.resolve_type(item)).collect())
                }
                Ty::Arrow(mut arrow) => {
                    arrow.input = Box::new(self.resolve_type(&arrow.input));
                    arrow.output = Box::new(self.resolve_type(&arrow.output));
                    arrow.functors = self.resolved_functor_argument(arrow.functors);
                    Ty::Arrow(arrow)
                }
                ty => ty,
            }
        }

        /// A generic type may use a narrower callable interface than the first
        /// value supplying evidence. Keep capabilities flexible until all values
        /// and consumers constrain them; the caller relates this view to `ty`.
        fn capability_view(&mut self, ty: &Ty) -> Ty {
            match ty {
                Ty::Array(item) => Ty::Array(Box::new(self.capability_view(item))),
                Ty::Tuple(items) => Ty::Tuple(
                    items
                        .iter()
                        .map(|item| self.capability_view(item))
                        .collect(),
                ),
                Ty::Arrow(arrow) => {
                    let mut arrow = arrow.clone();
                    arrow.input = Box::new(self.capability_view(&arrow.input));
                    arrow.output = Box::new(self.capability_view(&arrow.output));
                    if !matches!(arrow.functors, FunctorSet::Infer(_)) {
                        let param = self.new_param_id();
                        self.functor_bounds.insert(param, FunctorSetValue::Empty);
                        arrow.functors = FunctorSet::Param(param);
                    }
                    Ty::Arrow(arrow)
                }
                other => other.clone(),
            }
        }

        fn resolved_functor_argument(&self, functors: FunctorSet) -> FunctorSet {
            match functors {
                FunctorSet::Param(param) => self
                    .functor_bounds
                    .get(&param)
                    .map_or(FunctorSet::Param(param), |bound| FunctorSet::Value(*bound)),
                resolved => resolved,
            }
        }

        fn check_functors(&mut self, expected: FunctorSet, actual: FunctorSet) -> bool {
            if expected == actual {
                return true;
            }
            let mut pending = Vec::new();
            match (expected, actual) {
                (FunctorSet::Infer(_), _) | (_, FunctorSet::Infer(_)) => return true,
                (FunctorSet::Value(expected), FunctorSet::Value(actual)) => {
                    return actual.intersect(&expected) == expected;
                }
                (FunctorSet::Param(param), FunctorSet::Value(available)) => {
                    let limit = self
                        .functor_limits
                        .entry(param)
                        .or_insert(FunctorSetValue::CtlAdj);
                    let next = limit.intersect(&available);
                    if *limit != next {
                        *limit = next;
                        pending.push(param);
                    }
                }
                (FunctorSet::Value(required), FunctorSet::Param(param)) => {
                    let bound = self.functor_bounds.entry(param).or_default();
                    let next = bound.union(&required);
                    if *bound != next {
                        *bound = next;
                        pending.push(param);
                    }
                }
                (FunctorSet::Param(expected), FunctorSet::Param(actual)) => {
                    self.functor_requirements
                        .entry(expected)
                        .or_default()
                        .push(actual);
                    self.functor_dependents
                        .entry(actual)
                        .or_default()
                        .push(expected);
                    if let Some(required) = self.functor_bounds.get(&expected).copied() {
                        let bound = self.functor_bounds.entry(actual).or_default();
                        let next = bound.union(&required);
                        if *bound != next {
                            *bound = next;
                            pending.push(actual);
                        }
                    }
                    if let Some(available) = self.functor_limits.get(&actual).copied() {
                        let limit = self
                            .functor_limits
                            .entry(expected)
                            .or_insert(FunctorSetValue::CtlAdj);
                        let next = limit.intersect(&available);
                        if *limit != next {
                            *limit = next;
                            pending.push(expected);
                        }
                    }
                }
            }

            self.propagate_functor_bounds(pending)
        }

        fn propagate_functor_bounds(&mut self, mut pending: Vec<ParamId>) -> bool {
            // Each bound gains or loses at most two capabilities. Revisit only
            // connected variables when a bound changes.
            while let Some(param) = pending.pop() {
                if let Some(required) = self.functor_bounds.get(&param).copied() {
                    if self
                        .functor_limits
                        .get(&param)
                        .is_some_and(|available| available.intersect(&required) != required)
                    {
                        return false;
                    }
                    for &provided in self.functor_requirements.get(&param).into_iter().flatten() {
                        let bound = self.functor_bounds.entry(provided).or_default();
                        let next = bound.union(&required);
                        if *bound != next {
                            *bound = next;
                            pending.push(provided);
                        }
                    }
                }
                if let Some(available) = self.functor_limits.get(&param).copied() {
                    for &required in self.functor_dependents.get(&param).into_iter().flatten() {
                        let limit = self
                            .functor_limits
                            .entry(required)
                            .or_insert(FunctorSetValue::CtlAdj);
                        let next = limit.intersect(&available);
                        if *limit != next {
                            *limit = next;
                            pending.push(required);
                        }
                    }
                }
            }
            true
        }

        fn contains_param(&self, ty: &Ty, param: ParamId) -> bool {
            match self.resolve(ty) {
                Ty::Param(other) => param == other,
                Ty::Array(item) => self.contains_param(&item, param),
                Ty::Tuple(items) => items.iter().any(|item| self.contains_param(item, param)),
                Ty::Arrow(arrow) => {
                    self.contains_param(&arrow.input, param)
                        || self.contains_param(&arrow.output, param)
                }
                Ty::Err | Ty::Infer(_) | Ty::Prim(_) | Ty::Udt(_) => false,
            }
        }

        fn check_types(&mut self, expected: &Ty, actual: &Ty) -> Result<(), Vec<Error>> {
            self.check_types_with_variance(expected, actual, true)
        }

        fn check_types_with_variance(
            &mut self,
            expected: &Ty,
            actual: &Ty,
            covariant: bool,
        ) -> Result<(), Vec<Error>> {
            let expected = self.resolve(expected);
            let actual = self.resolve(actual);
            if expected == actual {
                return Ok(());
            }
            match (&expected, &actual) {
                (Ty::Err | Ty::Infer(_), _) | (_, Ty::Err | Ty::Infer(_)) => Ok(()),
                (Ty::Param(param), ty) if !self.contains_param(ty, *param) => {
                    let view = if self.nominal {
                        ty.clone()
                    } else {
                        self.capability_view(ty)
                    };
                    self.bindings.insert(*param, view.clone());
                    self.check_types_with_variance(&view, ty, covariant)
                }
                (ty, Ty::Param(param)) if !self.contains_param(ty, *param) => {
                    let view = if self.nominal {
                        ty.clone()
                    } else {
                        self.capability_view(ty)
                    };
                    self.bindings.insert(*param, view.clone());
                    self.check_types_with_variance(ty, &view, covariant)
                }
                (Ty::Array(expected), Ty::Array(actual)) => {
                    self.check_types_with_variance(expected, actual, covariant)
                }
                (Ty::Tuple(expected), Ty::Tuple(actual)) if expected.len() == actual.len() => {
                    for (expected, actual) in expected.iter().zip(actual) {
                        self.check_types_with_variance(expected, actual, covariant)?;
                    }
                    Ok(())
                }
                (Ty::Arrow(expected_arrow), Ty::Arrow(actual_arrow))
                    if expected_arrow.kind == actual_arrow.kind =>
                {
                    // A supplied callable must accept the input its caller can
                    // provide; callable inputs reverse capability requirements.
                    self.check_types_with_variance(
                        &expected_arrow.input,
                        &actual_arrow.input,
                        !covariant,
                    )?;
                    self.check_types_with_variance(
                        &expected_arrow.output,
                        &actual_arrow.output,
                        covariant,
                    )?;
                    let (required, available) = if covariant {
                        (expected_arrow.functors, actual_arrow.functors)
                    } else {
                        (actual_arrow.functors, expected_arrow.functors)
                    };
                    if self.nominal || self.check_functors(required, available) {
                        Ok(())
                    } else {
                        Err(vec![Error::InvalidRuntimeCallableArgumentType {
                            expected: Box::new(self.resolve_type(&expected)),
                            actual: Box::new(self.resolve_type(&actual)),
                        }])
                    }
                }
                _ => Err(vec![Error::InvalidRuntimeCallableArgumentType {
                    expected: Box::new(expected),
                    actual: Box::new(actual),
                }]),
            }
        }

        fn check_value(&mut self, expected: &Ty, value: &Value) -> Result<(), Vec<Error>> {
            let nominal_expected = if self.nominal {
                let ty = self.resolve(expected);
                if matches!(ty, Ty::Infer(_) | Ty::Err) {
                    return Ok(());
                }
                if matches!(ty, Ty::Param(_))
                    && !matches!(value, Value::Global(_, _) | Value::Closure(_))
                {
                    // Runtime values do not prove a nominal type: even tuple tags
                    // survive `Unwrap`. Only declared signatures bind these parameters.
                    self.deferred_values.push((ty, value.clone()));
                    return Ok(());
                }
                Some(self.udt_payload_ty(ty))
            } else {
                None
            };
            let expected = nominal_expected.as_ref().unwrap_or(expected);
            match value {
                Value::Tuple(values, _) => {
                    let items: Vec<_> = values.iter().map(|_| self.new_param()).collect();
                    self.check_types(expected, &Ty::Tuple(items.clone()))?;
                    for (expected, value) in items.iter().zip(values.iter()) {
                        self.check_value(expected, value)?;
                    }
                }
                Value::Array(values) => {
                    let item = self.new_param();
                    self.check_types(expected, &Ty::Array(Box::new(item.clone())))?;
                    for value in values.iter() {
                        self.check_value(&item, value)?;
                    }
                }
                Value::Global(id, functor) => {
                    let info = self.callables.get(id).expect("runtime callable type");
                    let ty = self.fresh_type(
                        if self.nominal {
                            &info.declared_ty
                        } else {
                            &info.ty
                        },
                        &info.generics,
                    );
                    self.check_callable(expected, &runtime_callable_ty(ty, *functor))?;
                }
                Value::Closure(closure) => {
                    let info = self
                        .callables
                        .get(&closure.id)
                        .expect("runtime closure type");
                    let full_ty = self.fresh_type(
                        if self.nominal {
                            &info.declared_ty
                        } else {
                            &info.ty
                        },
                        &info.generics,
                    );
                    if let Some(captures) =
                        closure_capture_ty_hints(&full_ty, closure.fixed_args.len())
                    {
                        for (expected, value) in captures.iter().zip(closure.fixed_args.iter()) {
                            self.check_value(expected, value)?;
                        }
                    }
                    let ty = partial_applied_closure_ty(&full_ty, closure.fixed_args.len());
                    self.check_callable(expected, &runtime_callable_ty(ty, closure.functor))?;
                }
                // A dynamic value does not establish a type. Keep checking its siblings.
                Value::Var(_) => {}
                _ => {
                    if let Some(actual) = primitive_value_ty(value) {
                        self.check_types(expected, &actual)?;
                    }
                }
            }
            Ok(())
        }

        /// Rechecks values whose expected nominal type was unknown when first visited.
        ///
        /// For example, `Forward<'T>(value : 'T, read : 'T -> Int)` might receive
        /// `[IdA]` followed by `ReadArrayB`, where `IdA` is `A -> A` and `ReadArrayB` expects
        /// an array of B -> B callbacks. The array is initially deferred: its runtime
        /// representation cannot establish `'T`'s nominal type. `ReadArrayB`'s declared
        /// signature then binds `'T` to `(B -> B)[]`, allowing this recheck to inspect
        /// the array and reject `IdA` even if A and B have identical payload layouts.
        ///
        /// Each round takes the current queue so `check_value` can enqueue unresolved
        /// values or nested children separately. Inspecting a newly resolved aggregate
        /// can reveal callbacks that add bindings needed by other deferred values,
        /// so another round may be necessary.
        ///
        /// Bindings only grow. If a complete round adds none, the remaining values
        /// still lack nominal evidence; stop rather than loop forever or infer named
        /// types from runtime payloads. The separate structural pass remains responsible
        /// for checking their runtime representation.
        fn check_deferred_values(&mut self) -> Result<(), Vec<Error>> {
            while !self.deferred_values.is_empty() {
                let previous_bindings = self.bindings.len();
                for (expected, value) in std::mem::take(&mut self.deferred_values) {
                    self.check_value(&expected, &value)?;
                }
                // Unconstrained payloads remain unknown; bindings only grow.
                if self.bindings.len() == previous_bindings {
                    break;
                }
            }
            Ok(())
        }

        fn check_callable(&mut self, expected: &Ty, actual: &Ty) -> Result<(), Vec<Error>> {
            let expected = self.udt_payload_ty(self.resolve(expected));
            // Synthetic preparation also accepts a bare callable for the first callable
            // position of a tuple, leaving non-callable positions as placeholders.
            if let Ty::Tuple(items) = &expected
                && let Some(item) = items
                    .iter()
                    .find(|ty| ty_is_arrow_or_contains_arrow(&resolve_udt_ty(self.fir_store, ty)))
            {
                return self.check_callable(item, actual);
            }
            self.check_types(&expected, actual)
        }

        fn udt_payload_ty(&self, mut ty: Ty) -> Ty {
            // Only expand the value's outer wrapper, not UDTs inside callback
            // signatures: equal payload layouts do not imply equal declared types.
            while let Ty::Udt(qsc_fir::fir::Res::Item(id)) = ty {
                let item = self
                    .fir_store
                    .get(id.package)
                    .items
                    .get(id.item)
                    .expect("UDT item should exist");
                let qsc_fir::fir::ItemKind::Ty(_, udt) = &item.kind else {
                    break;
                };
                ty = udt.get_pure_ty();
            }
            ty
        }
    }

    fn runtime_callable_ty(mut ty: Ty, functor: FunctorApp) -> Ty {
        let Ty::Arrow(arrow) = &mut ty else {
            unreachable!("runtime callable should have an arrow type");
        };
        for _ in 0..functor.controlled {
            *arrow.input = Ty::Tuple(vec![
                Ty::Array(Box::new(Ty::Prim(qsc_fir::ty::Prim::Qubit))),
                *arrow.input.clone(),
            ]);
        }
        ty
    }

    fn validate_runtime_callable_args(
        fir_store: &qsc_fir::fir::PackageStore,
        target: qsc_fir::fir::StoreItemId,
        args: &Value,
        callables: &FxHashMap<qsc_fir::fir::StoreItemId, CallableValueInfo>,
    ) -> Result<FxHashMap<ParamId, GenericArg>, Vec<Error>> {
        use qsc_fir::fir::{Global, PackageLookup};

        let package = fir_store.get(target.package);
        let Some(Global::Callable(decl)) = package.get_global(target.item) else {
            panic!("target should be callable");
        };
        let (_, ty) = callable_expr_span_and_ty(fir_store, target);
        let Ty::Arrow(arrow) = ty else {
            unreachable!("target should have an arrow type");
        };
        // Keep declaration identity separate from erased runtime representation.
        // In particular, a capture cannot prove a generic nominal type.
        let mut inferred_args = FxHashMap::default();
        for nominal in [true, false] {
            let mut checker = RuntimeArgTypeChecker {
                fir_store,
                callables,
                nominal,
                bindings: FxHashMap::default(),
                functor_requirements: FxHashMap::default(),
                functor_dependents: FxHashMap::default(),
                functor_limits: FxHashMap::default(),
                functor_bounds: decl
                    .generics
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, parameter)| match parameter {
                        qsc_fir::ty::TypeParameter::Functor(bound) => {
                            Some((ParamId::from(idx), *bound))
                        }
                        qsc_fir::ty::TypeParameter::Ty { .. } => None,
                    })
                    .collect(),
                deferred_values: Vec::new(),
                next_param: decl.generics.len(),
            };
            let input = if nominal {
                *arrow.input.clone()
            } else {
                resolve_udt_ty(fir_store, &arrow.input)
            };
            // Reserve target IDs before freshening each supplied callable's scheme.
            checker.check_value(&input, args)?;
            if nominal {
                checker.check_deferred_values()?;
            } else {
                for (idx, parameter) in decl.generics.iter().enumerate() {
                    let param = ParamId::from(idx);
                    let argument = match parameter {
                        qsc_fir::ty::TypeParameter::Ty { .. } => {
                            GenericArg::Ty(checker.resolve_type(&Ty::Param(param)))
                        }
                        qsc_fir::ty::TypeParameter::Functor(_) => GenericArg::Functor(
                            checker.resolved_functor_argument(FunctorSet::Param(param)),
                        ),
                    };
                    inferred_args.insert(param, argument);
                }
            }
        }
        Ok(inferred_args)
    }

    /// Preserves the structural signatures used to reconstruct runtime UDT values.
    ///
    /// Runtime values carry tuple payloads rather than nominal UDT types. Keep
    /// concrete input/output signatures aligned with those payloads before the
    /// synthetic entry is transformed. Generic signatures remain available for
    /// monomorphization; functor bounds were already resolved in HIR.
    fn normalize_runtime_callable_udts(
        fir_store: &mut qsc_fir::fir::PackageStore,
        callables: &FxHashSet<qsc_fir::fir::StoreItemId>,
    ) {
        use qsc_fir::fir::{Global, PackageLookup};

        let normalized: Vec<_> = callables
            .iter()
            .map(|id| {
                let package = fir_store.get(id.package);
                let Some(Global::Callable(callable_decl)) = package.get_global(id.item) else {
                    panic!("callable should exist in lowered package");
                };
                let normalized_signature = if callable_decl.generics.is_empty() {
                    let input_pat = package.get_pat(callable_decl.input);
                    Some((
                        resolve_udt_ty(fir_store, &input_pat.ty),
                        resolve_udt_ty(fir_store, &callable_decl.output),
                    ))
                } else {
                    None
                };
                (*id, callable_decl.input, normalized_signature)
            })
            .collect();

        for (id, input_pat_id, normalized_signature) in normalized {
            let Some((input_ty, output_ty)) = normalized_signature else {
                continue;
            };
            let package = fir_store.get_mut(id.package);
            package
                .pats
                .get_mut(input_pat_id)
                .expect("callable input pattern should exist")
                .ty = input_ty;
            let qsc_fir::fir::ItemKind::Callable(callable_decl) = &mut package
                .items
                .get_mut(id.item)
                .expect("callable item should exist")
                .kind
            else {
                panic!("callable should exist in lowered package");
            };
            callable_decl.output = output_ty;
        }
    }

    /// Closes inherited functor parameters in concrete runtime declarations.
    ///
    /// Lifted lambdas can retain functor parameters even without declared generics.
    /// HIR still carries their checked lower bounds; FIR retains only parameter
    /// IDs. Resolve those bounds before lowering, without specializing shared
    /// declarations to individual runtime values. Generic templates are left for
    /// monomorphization, and packages are copied only when a concrete item is selected.
    pub(super) fn normalize_runtime_callable_hir<'a>(
        package: &'a qsc_hir::hir::Package,
        package_id: qsc_fir::fir::PackageId,
        callables: &FxHashSet<qsc_fir::fir::StoreItemId>,
    ) -> std::borrow::Cow<'a, qsc_hir::hir::Package> {
        use qsc_hir::mut_visit::MutVisitor;
        use std::borrow::Cow;

        let mut normalized = Cow::Borrowed(package);
        for id in callables.iter().filter(|id| id.package == package_id) {
            let local_id = qsc_lowerer::map_fir_local_item_to_hir(id.item);
            let item = package
                .items
                .get(local_id)
                .expect("runtime callable should exist");
            let qsc_hir::hir::ItemKind::Callable(decl) = &item.kind else {
                panic!("runtime item should be callable");
            };
            if decl.generics.is_empty() {
                RuntimeFunctorNormalizer.visit_item(
                    normalized
                        .to_mut()
                        .items
                        .get_mut(local_id)
                        .expect("runtime callable should exist"),
                );
            }
        }
        normalized
    }

    struct RuntimeFunctorNormalizer;

    impl RuntimeFunctorNormalizer {
        fn normalize_ty(ty: &qsc_hir::ty::Ty) -> qsc_hir::ty::Ty {
            use qsc_hir::ty::{Arrow, FunctorSet, Ty};
            use std::{cell::RefCell, rc::Rc};

            match ty {
                Ty::Arrow(arrow) => {
                    let functors = match *arrow.functors.borrow() {
                        FunctorSet::Param(_, bound) => FunctorSet::Value(bound),
                        other => other,
                    };
                    // HIR clones share arrow cells. Build fresh arrows all the
                    // way down so normalization cannot mutate the interpreter.
                    Ty::Arrow(Rc::new(Arrow {
                        kind: arrow.kind,
                        input: RefCell::new(Self::normalize_ty(&arrow.input.borrow())),
                        output: RefCell::new(Self::normalize_ty(&arrow.output.borrow())),
                        functors: RefCell::new(functors),
                    }))
                }
                Ty::Array(item) => Ty::Array(Box::new(Self::normalize_ty(item))),
                Ty::Tuple(items) => Ty::Tuple(items.iter().map(Self::normalize_ty).collect()),
                _ => ty.clone(),
            }
        }
    }

    impl qsc_hir::mut_visit::MutVisitor for RuntimeFunctorNormalizer {
        fn visit_callable_decl(&mut self, decl: &mut qsc_hir::hir::CallableDecl) {
            decl.output = Self::normalize_ty(&decl.output);
            qsc_hir::mut_visit::walk_callable_decl(self, decl);
        }

        fn visit_pat(&mut self, pat: &mut qsc_hir::hir::Pat) {
            pat.ty = Self::normalize_ty(&pat.ty);
            qsc_hir::mut_visit::walk_pat(self, pat);
        }

        fn visit_expr(&mut self, expr: &mut qsc_hir::hir::Expr) {
            expr.ty = Self::normalize_ty(&expr.ty);
            if let qsc_hir::hir::ExprKind::Var(_, args) = &mut expr.kind {
                for arg in args {
                    match arg {
                        qsc_hir::ty::GenericArg::Ty(ty) => *ty = Self::normalize_ty(ty),
                        qsc_hir::ty::GenericArg::Functor(functors) => {
                            if let qsc_hir::ty::FunctorSet::Param(_, bound) = *functors {
                                *functors = qsc_hir::ty::FunctorSet::Value(bound);
                            }
                        }
                    }
                }
            }
            qsc_hir::mut_visit::walk_expr(self, expr);
        }

        fn visit_block(&mut self, block: &mut qsc_hir::hir::Block) {
            block.ty = Self::normalize_ty(&block.ty);
            qsc_hir::mut_visit::walk_block(self, block);
        }
    }

    /// Seeds the package entry with a synthetic `Call(target, args)` expression.
    ///
    /// Builds args matching the target callable's pure input type: callable-typed positions
    /// are filled with Var references to the concrete callables from the `args` Value;
    /// non-callable positions get typed placeholder literals (which are never evaluated —
    /// they exist only to make the Call structurally valid for defunctionalization).
    #[allow(clippy::too_many_lines)]
    fn seed_entry_with_call_to_target(
        fir_store: &mut qsc_fir::fir::PackageStore,
        fir_package_id: qsc_fir::fir::PackageId,
        target_callable: qsc_fir::fir::StoreItemId,
        args: &Value,
        callable_types: &rustc_hash::FxHashMap<qsc_fir::fir::StoreItemId, CallableValueInfo>,
        inferred_args: &FxHashMap<ParamId, GenericArg>,
    ) -> Result<(), qsc_passes::Error> {
        use qsc_fir::fir::{Global, PackageLookup};

        // Pre-compute target's arrow type and input pattern type (immutable borrow of store).
        let package = fir_store.get(target_callable.package);
        let Some(Global::Callable(callable_decl)) = package.get_global(target_callable.item) else {
            panic!("target callable must exist in lowered package");
        };
        let span = callable_decl.span;
        let input_pat = package.get_pat(callable_decl.input);
        let formal_input_ty = resolve_udt_ty(fir_store, &input_pat.ty);
        let formal_output_ty = resolve_udt_ty(fir_store, &callable_decl.output);
        let (generic_args, input_ty, output_ty, arrow_ty) = instantiate_synthetic_target_arrow(
            callable_decl.generics.as_slice(),
            qsc_fir::ty::Arrow {
                kind: callable_decl.kind,
                input: Box::new(formal_input_ty),
                output: Box::new(formal_output_ty),
                functors: qsc_fir::ty::FunctorSet::Value(callable_decl.functors),
            },
            inferred_args,
        )
        .ok_or(qsc_passes::Error::EntryPoint(
            qsc_passes::EntryPointError::MissingTypeArguments(callable_decl.name.span.span),
        ))?;

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
            &input_ty,
            args,
            callable_types,
            &mut pending_stmts,
        );

        // Create callee Var expression referencing the target callable.
        let callee_expr_id = assigner.next_expr();
        package.exprs.insert(
            callee_expr_id,
            qsc_fir::fir::Expr {
                id: callee_expr_id,
                span,
                ty: arrow_ty,
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
        Ok(())
    }

    /// Infers concrete generic arguments for the synthetic target invocation and
    /// returns the target's instantiated input, output, and arrow types.
    ///
    /// The synthetic entry is built before the normal monomorphization pass can
    /// specialize the target for these runtime arguments. Instantiating the
    /// arrow here keeps the synthetic call structurally concrete, so later FIR
    /// passes do not see unresolved type or functor parameters.
    fn instantiate_synthetic_target_arrow(
        generics: &[qsc_fir::ty::TypeParameter],
        formal_arrow: qsc_fir::ty::Arrow,
        inferred_args: &FxHashMap<ParamId, GenericArg>,
    ) -> Option<(
        Vec<qsc_fir::ty::GenericArg>,
        qsc_fir::ty::Ty,
        qsc_fir::ty::Ty,
        qsc_fir::ty::Ty,
    )> {
        let generic_args = infer_target_generic_args(generics, inferred_args)?;
        let instantiated_arrow =
            qsc_fir::ty::Scheme::new(generics.to_vec(), Box::new(formal_arrow))
                .instantiate(&generic_args)
                .expect("inferred arguments should match the target's generic parameters");
        let input_ty = resolve_functor_params(&instantiated_arrow.input);
        let output_ty = resolve_functor_params(&instantiated_arrow.output);
        let arrow_ty = qsc_fir::ty::Ty::Arrow(Box::new(qsc_fir::ty::Arrow {
            kind: instantiated_arrow.kind,
            input: Box::new(input_ty.clone()),
            output: Box::new(output_ty.clone()),
            functors: instantiated_arrow.functors,
        }));
        Some((generic_args, input_ty, output_ty, arrow_ty))
    }

    /// Builds an args expression matching the target's input type.
    ///
    /// For callable-typed positions, uses the corresponding callable from `args`.
    /// For non-callable positions, uses `lower_value_to_expr` if the value is available
    /// in `args`, otherwise creates a typed placeholder literal.
    #[allow(clippy::too_many_lines)]
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
                // Multi-param input — walk each position.
                // If args is a Tuple of same length, pair element-wise.
                // Otherwise, match the first callable-typed position to args.
                let arg_elems: Vec<&Value> = match args {
                    Value::Tuple(vs, _) if vs.len() == elem_tys.len() => vs.iter().collect(),
                    _ => {
                        // Args doesn't match tuple structure — build with
                        // args placed at the first arrow-typed position.
                        let mut elem_ids = Vec::with_capacity(elem_tys.len());
                        let mut args_used = false;
                        for elem_ty in elem_tys {
                            if !args_used && ty_is_arrow_or_contains_arrow(elem_ty) {
                                elem_ids.push(lower_value_to_expr(
                                    package,
                                    assigner,
                                    args,
                                    Some(elem_ty),
                                    callable_types,
                                    pending_stmts,
                                ));
                                args_used = true;
                            } else {
                                elem_ids.push(make_placeholder_expr(package, assigner, elem_ty));
                            }
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
                        return expr_id;
                    }
                };

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
            qsc_fir::ty::Ty::Arrow(_) => {
                // Arrow-typed position — the args must be a callable value.
                lower_value_to_expr(
                    package,
                    assigner,
                    args,
                    Some(input_ty),
                    callable_types,
                    pending_stmts,
                )
            }
            qsc_fir::ty::Ty::Array(_) => {
                // Array position — lower the value, threading the declared element
                // type so empty (and nested-empty) arrays carry their real element
                // type instead of `Ty::Err`.
                lower_value_to_expr(
                    package,
                    assigner,
                    args,
                    Some(input_ty),
                    callable_types,
                    pending_stmts,
                )
            }
            _ => {
                // Non-callable position — lower value if possible, otherwise placeholder.
                match args {
                    Value::Qubit(_) | Value::Var(_) => {
                        make_placeholder_expr(package, assigner, input_ty)
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

    /// Returns true if the type is an Arrow or contains an Arrow in tuple structure.
    fn ty_is_arrow_or_contains_arrow(ty: &qsc_fir::ty::Ty) -> bool {
        match ty {
            qsc_fir::ty::Ty::Arrow(_) => true,
            qsc_fir::ty::Ty::Tuple(elems) => elems.iter().any(ty_is_arrow_or_contains_arrow),
            _ => false,
        }
    }

    /// Creates a typed placeholder expression for a non-callable input position.
    ///
    /// Uses `Lit(Int(0))` with the declared type. The placeholder is never evaluated —
    /// it exists only to make the synthetic Call structurally valid for pipeline passes.
    fn make_placeholder_expr(
        package: &mut qsc_fir::fir::Package,
        assigner: &mut qsc_fir::assigner::Assigner,
        ty: &qsc_fir::ty::Ty,
    ) -> qsc_fir::fir::ExprId {
        let expr_id = assigner.next_expr();
        package.exprs.insert(
            expr_id,
            qsc_fir::fir::Expr {
                id: expr_id,
                span: package.synthetic_span(),
                ty: ty.clone(),
                kind: qsc_fir::fir::ExprKind::Lit(qsc_fir::fir::Lit::Int(0)),
                exec_graph_range: qsc_fir::fir::ExecGraphIdx::ZERO
                    ..qsc_fir::fir::ExecGraphIdx::ZERO,
            },
        );
        expr_id
    }

    /// Resolves `FunctorSet::Param` to `FunctorSet::Value(Empty)` recursively in a type.
    ///
    /// Retains the existing fallback for synthetic value and target type construction.
    /// Concrete runtime declarations preserve their checked bounds separately, in HIR.
    fn resolve_functor_params(ty: &qsc_fir::ty::Ty) -> qsc_fir::ty::Ty {
        match ty {
            qsc_fir::ty::Ty::Arrow(arrow) => {
                let functors = match arrow.functors {
                    qsc_fir::ty::FunctorSet::Param(_) | qsc_fir::ty::FunctorSet::Infer(_) => {
                        qsc_fir::ty::FunctorSet::Value(qsc_fir::ty::FunctorSetValue::Empty)
                    }
                    other @ qsc_fir::ty::FunctorSet::Value(_) => other,
                };
                qsc_fir::ty::Ty::Arrow(Box::new(qsc_fir::ty::Arrow {
                    kind: arrow.kind,
                    input: Box::new(resolve_functor_params(&arrow.input)),
                    output: Box::new(resolve_functor_params(&arrow.output)),
                    functors,
                }))
            }
            qsc_fir::ty::Ty::Tuple(elems) => {
                qsc_fir::ty::Ty::Tuple(elems.iter().map(resolve_functor_params).collect())
            }
            qsc_fir::ty::Ty::Array(inner) => {
                qsc_fir::ty::Ty::Array(Box::new(resolve_functor_params(inner)))
            }
            other => other.clone(),
        }
    }

    /// Collects only fully resolved arguments from the shared type/functor solution.
    /// Missing type evidence remains an error; functor lower bounds are resolved
    /// by the checker before reaching this boundary.
    fn infer_target_generic_args(
        generics: &[qsc_fir::ty::TypeParameter],
        inferred_args: &FxHashMap<ParamId, GenericArg>,
    ) -> Option<Vec<qsc_fir::ty::GenericArg>> {
        generics
            .iter()
            .enumerate()
            .map(|(idx, param)| {
                let inferred = inferred_args.get(&ParamId::from(idx));
                match (param, inferred) {
                    (qsc_fir::ty::TypeParameter::Ty { .. }, Some(GenericArg::Ty(ty)))
                        if !ty_is_unresolved(ty) =>
                    {
                        Some(GenericArg::Ty(ty.clone()))
                    }
                    (
                        qsc_fir::ty::TypeParameter::Functor(_),
                        Some(GenericArg::Functor(functors @ FunctorSet::Value(_))),
                    ) => Some(GenericArg::Functor(*functors)),
                    _ => None,
                }
            })
            .collect()
    }

    /// Returns true when a type still contains unresolved type or functor information.
    fn ty_is_unresolved(ty: &qsc_fir::ty::Ty) -> bool {
        match ty {
            qsc_fir::ty::Ty::Param(_) | qsc_fir::ty::Ty::Infer(_) | qsc_fir::ty::Ty::Err => true,
            qsc_fir::ty::Ty::Array(item) => ty_is_unresolved(item),
            qsc_fir::ty::Ty::Arrow(arrow) => {
                !matches!(arrow.functors, qsc_fir::ty::FunctorSet::Value(_))
                    || ty_is_unresolved(&arrow.input)
                    || ty_is_unresolved(&arrow.output)
            }
            qsc_fir::ty::Ty::Tuple(items) => items.iter().any(ty_is_unresolved),
            qsc_fir::ty::Ty::Prim(_) | qsc_fir::ty::Ty::Udt(_) => false,
        }
    }

    fn primitive_value_ty(value: &Value) -> Option<Ty> {
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
            Value::Tuple(_, _)
            | Value::Array(_)
            | Value::Global(_, _)
            | Value::Closure(_)
            | Value::Var(_) => None,
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
                return lower_global_to_expr(
                    package,
                    assigner,
                    *id,
                    *functor,
                    expected_ty,
                    callable_types,
                );
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
        expected_ty: Option<&qsc_fir::ty::Ty>,
        callable_types: &rustc_hash::FxHashMap<qsc_fir::fir::StoreItemId, CallableValueInfo>,
    ) -> qsc_fir::fir::ExprId {
        let info = callable_types
            .get(&id)
            .expect("Global callable type must be pre-computed");
        let (ty, generic_args) = expected_ty
            .and_then(|expected| instantiate_runtime_global(info, functor, expected))
            .unwrap_or_else(|| (info.ty.clone(), Vec::new()));
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
                    generic_args,
                ),
                exec_graph_range: qsc_fir::fir::ExecGraphIdx::ZERO
                    ..qsc_fir::fir::ExecGraphIdx::ZERO,
            },
        );
        wrap_expr_with_functor_app(package, assigner, expr_id, &ty, functor)
    }

    /// Infers generic arguments through the runtime functor's input layers,
    /// then instantiates the original declaration before those layers are added
    /// to the FIR expression. The declaration retains its own outer functors,
    /// independently of a consumer's weaker or parameterized functor requirement.
    ///
    /// Returns `None` for nongeneric callables or incompatible input/output shapes.
    fn instantiate_runtime_global(
        info: &CallableValueInfo,
        functor: FunctorApp,
        expected_ty: &Ty,
    ) -> Option<(Ty, Vec<qsc_fir::ty::GenericArg>)> {
        if info.generics.is_empty() {
            return None;
        }
        let Ty::Arrow(expected) = expected_ty else {
            return None;
        };
        let Ty::Arrow(formal) = &info.ty else {
            unreachable!("global callable should have an arrow type");
        };
        let Ty::Arrow(applied) = runtime_callable_ty(info.ty.clone(), functor) else {
            unreachable!("functor-applied callable should have an arrow type");
        };
        let mut arg_map = rustc_hash::FxHashMap::default();
        if applied.kind != expected.kind
            || !infer_generic_ty_args(&applied.input, &expected.input, &mut arg_map)
            || !infer_generic_ty_args(&applied.output, &expected.output, &mut arg_map)
        {
            return None;
        }
        let generic_args = info
            .generics
            .iter()
            .enumerate()
            .map(
                |(idx, param)| match (param, arg_map.get(&qsc_fir::ty::ParamId::from(idx))) {
                    (
                        qsc_fir::ty::TypeParameter::Ty { .. },
                        Some(qsc_fir::ty::GenericArg::Ty(ty)),
                    ) => Some(qsc_fir::ty::GenericArg::Ty(ty.clone())),
                    (
                        qsc_fir::ty::TypeParameter::Functor(_),
                        Some(qsc_fir::ty::GenericArg::Functor(functors)),
                    ) => Some(qsc_fir::ty::GenericArg::Functor(*functors)),
                    _ => None,
                },
            )
            .collect::<Option<Vec<_>>>()?;
        let instantiated = qsc_fir::ty::Scheme::new(info.generics.clone(), formal.clone())
            .instantiate(&generic_args)
            .expect("inferred arguments should match the declaration's generic parameters");
        Some((Ty::Arrow(Box::new(instantiated)), generic_args))
    }

    /// Structurally unifies a formal type against an actual type, recording each
    /// type/functor parameter binding in `arg_map`.
    ///
    /// Returns `false` on any structural mismatch or on conflicting bindings for
    /// the same parameter (see [`record_inferred_arg`]).
    fn infer_generic_ty_args(
        formal: &qsc_fir::ty::Ty,
        actual: &qsc_fir::ty::Ty,
        arg_map: &mut rustc_hash::FxHashMap<qsc_fir::ty::ParamId, qsc_fir::ty::GenericArg>,
    ) -> bool {
        match (formal, actual) {
            // An empty runtime array may only repeat its expected element type.
            // That is not evidence; a sibling argument can still constrain it.
            (qsc_fir::ty::Ty::Param(_), _) if ty_is_unresolved(actual) => true,
            (qsc_fir::ty::Ty::Param(param), _) => {
                record_inferred_arg(*param, qsc_fir::ty::GenericArg::Ty(actual.clone()), arg_map)
            }
            (qsc_fir::ty::Ty::Array(formal), qsc_fir::ty::Ty::Array(actual)) => {
                infer_generic_ty_args(formal, actual, arg_map)
            }
            (qsc_fir::ty::Ty::Arrow(formal), qsc_fir::ty::Ty::Arrow(actual)) => {
                formal.kind == actual.kind
                    && infer_generic_ty_args(&formal.input, &actual.input, arg_map)
                    && infer_generic_ty_args(&formal.output, &actual.output, arg_map)
                    && infer_generic_functor_args(formal.functors, actual.functors, arg_map)
            }
            (qsc_fir::ty::Ty::Tuple(formal), qsc_fir::ty::Ty::Tuple(actual))
                if formal.len() == actual.len() =>
            {
                formal
                    .iter()
                    .zip(actual)
                    .all(|(formal, actual)| infer_generic_ty_args(formal, actual, arg_map))
            }
            (qsc_fir::ty::Ty::Prim(formal), qsc_fir::ty::Ty::Prim(actual)) => formal == actual,
            (qsc_fir::ty::Ty::Udt(formal), qsc_fir::ty::Ty::Udt(actual)) => formal == actual,
            (qsc_fir::ty::Ty::Infer(formal), qsc_fir::ty::Ty::Infer(actual)) => formal == actual,
            (qsc_fir::ty::Ty::Err, qsc_fir::ty::Ty::Err) => true,
            _ => false,
        }
    }

    /// Unifies a formal functor set against an actual one, recording the binding
    /// when the formal side is a functor parameter; otherwise requires the two
    /// sets to be equal.
    fn infer_generic_functor_args(
        formal: qsc_fir::ty::FunctorSet,
        actual: qsc_fir::ty::FunctorSet,
        arg_map: &mut rustc_hash::FxHashMap<qsc_fir::ty::ParamId, qsc_fir::ty::GenericArg>,
    ) -> bool {
        match formal {
            qsc_fir::ty::FunctorSet::Param(param) => {
                record_inferred_arg(param, qsc_fir::ty::GenericArg::Functor(actual), arg_map)
            }
            _ => formal == actual,
        }
    }

    /// Records the inferred argument for a generic parameter, returning whether
    /// it is consistent: a first binding is inserted and accepted, while a
    /// repeated binding must equal the one already recorded.
    fn record_inferred_arg(
        param: qsc_fir::ty::ParamId,
        arg: qsc_fir::ty::GenericArg,
        arg_map: &mut rustc_hash::FxHashMap<qsc_fir::ty::ParamId, qsc_fir::ty::GenericArg>,
    ) -> bool {
        if let Some(existing) = arg_map.get(&param) {
            existing == &arg
        } else {
            arg_map.insert(param, arg);
            true
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
                ty,
                qsc_fir::fir::Functor::Adj,
            );
        }
        for _ in 0..functor.controlled {
            current_ty = runtime_callable_ty(
                current_ty,
                FunctorApp {
                    adjoint: false,
                    controlled: 1,
                },
            );
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
    /// Captures are bound to fresh locals and referenced by an `ExprKind::Closure`.
    /// A capture-free value can be a direct item reference only when its public
    /// input matches the implementation input. Lifted lambdas retain the closure
    /// wrapper that packs the explicit input into the implementation's tuple.
    /// Runtime functors are preserved in either representation.
    fn lower_closure_to_expr(
        package: &mut qsc_fir::fir::Package,
        assigner: &mut qsc_fir::assigner::Assigner,
        closure: &qsc_eval::val::Closure,
        callable_types: &rustc_hash::FxHashMap<qsc_fir::fir::StoreItemId, CallableValueInfo>,
        pending_stmts: &mut Vec<qsc_fir::fir::StmtId>,
    ) -> qsc_fir::fir::ExprId {
        // Lifted lambdas use `(captures.., explicit_input)`, even with no captures.
        let full_ty = callable_types
            .get(&closure.id)
            .expect("Closure callable type must be pre-computed")
            .ty
            .clone();

        let closure_ty = partial_applied_closure_ty(&full_ty, closure.fixed_args.len());
        if closure.fixed_args.is_empty() && closure_ty == full_ty {
            let expr_id = assigner.next_expr();
            package.exprs.insert(
                expr_id,
                qsc_fir::fir::Expr {
                    id: expr_id,
                    span: package.synthetic_span(),
                    ty: full_ty.clone(),
                    kind: qsc_fir::fir::ExprKind::Var(
                        qsc_fir::fir::Res::Item(qsc_fir::fir::ItemId {
                            package: closure.id.package,
                            item: closure.id.item,
                        }),
                        Vec::new(),
                    ),
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

        // Materialize each capture as an in-scope local, then build an
        // `ExprKind::Closure` value referencing those locals.
        let capture_ty_hints = closure_capture_ty_hints(&full_ty, closure.fixed_args.len());
        let mut capture_locals = Vec::with_capacity(closure.fixed_args.len());
        for (idx, capture) in closure.fixed_args.iter().enumerate() {
            let value_expr_id = lower_value_to_expr(
                package,
                assigner,
                capture,
                capture_ty_hints.as_ref().map(|types| &types[idx]),
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

    /// Computes the externally visible arrow type for a closure value.
    ///
    /// The lifted callable's input is the tuple `(captures.., explicit_input)`;
    /// dropping the leading captures yields the closure type the target parameter
    /// expects. The explicit input occupies a single trailing slot, so a one-element
    /// remainder is unwrapped back to that element's type.
    fn partial_applied_closure_ty(
        full_ty: &qsc_fir::ty::Ty,
        capture_count: usize,
    ) -> qsc_fir::ty::Ty {
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
            other if capture_count == 0 => other.clone(),
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
    /// args can be represented as FIR values, making the target and args
    /// entry-reachable for full pipeline participation. Falls back to a
    /// pin-based approach when args contain runtime identities that cannot be
    /// represented as FIR values.
    ///
    /// The original target is pinned for DCE survival so that `fir_to_qir_from_callable`
    /// can still use the original ID for partial evaluation.
    /// Rejects provable callable argument type mismatches before transforms.
    /// Preserves UDT identity from declarations separately
    /// from the structural checks of erased runtime payloads.
    /// This is not complete runtime validation: unresolved types, type-class
    /// constraints, and argument functor support retain their existing checks.
    #[allow(clippy::too_many_lines)]
    pub fn prepare_codegen_fir_from_callable_args(
        package_store: &PackageStore,
        callable: qsc_hir::hir::ItemId,
        args: &Value,
        capabilities: TargetCapabilityFlags,
    ) -> Result<(CodegenFir, CallableArgsBackend), Vec<Error>> {
        let mut concrete_callables = FxHashSet::default();
        collect_concrete_qsharp_callables(args, &mut concrete_callables);

        let target_callable = qsc_fir::fir::StoreItemId {
            package: qsc_lowerer::map_hir_package_to_fir(callable.package),
            item: qsc_lowerer::map_hir_local_item_to_fir(callable.item),
        };

        if concrete_callables.is_empty() {
            let codegen_fir =
                prepare_codegen_fir_from_callable(package_store, callable, capabilities)?;
            return Ok((
                codegen_fir,
                CallableArgsBackend::ReinvokeOriginal {
                    callable: target_callable,
                    args: args.clone(),
                },
            ));
        }

        // Runtime identities and foreign closures keep their original values
        // through reinvocation. FIR closures have package-local targets, so
        // reconstructing a foreign closure in this package would change its identity.
        if !value_is_fir_lowerable(args, target_callable.package) {
            let codegen_fir = prepare_codegen_fir_from_callable_args_pinned(
                package_store,
                callable,
                args,
                capabilities,
                concrete_callables,
            )?;
            return Ok((
                codegen_fir,
                CallableArgsBackend::ReinvokeOriginal {
                    callable: target_callable,
                    args: args.clone(),
                },
            ));
        }

        let (mut fir_store, fir_package_id, _assigner) = lower_to_fir(
            package_store,
            callable.package,
            None,
            Some(&concrete_callables),
        );

        // Concrete functor bounds are already closed in HIR. Cache callable value
        // types before adapting the nongeneric UDT signatures to runtime payloads;
        // generic declarations remain available for monomorphization.
        let callable_types = build_callable_type_map(&fir_store, &concrete_callables);
        let inferred_args =
            validate_runtime_callable_args(&fir_store, target_callable, args, &callable_types)?;
        normalize_runtime_callable_udts(&mut fir_store, &concrete_callables);

        // Build synthetic Call(Var(target), args) as the entry expression.
        // This makes the target and all callable args entry-reachable for pipeline transforms.
        seed_entry_with_call_to_target(
            &mut fir_store,
            fir_package_id,
            target_callable,
            args,
            &callable_types,
            &inferred_args,
        )
        .map_err(|error| map_pass_errors(package_store, callable.package, vec![error]))?;

        // FIR-lowerable callable values — whether passed directly, captured by a
        // closure, or wrapped inside a UDT field — lower into a self-contained
        // synthetic entry that is evaluated directly. Field-typed callables hidden
        // inside a UDT collapse during defunctionalization and UDT erasure so the
        // entry's argument shape stays aligned with the specialized body.
        let backend = CallableArgsBackend::SyntheticEntry;

        // The self-contained synthetic entry consumes the specialized clone
        // directly, so the original target is free to be removed by dead-code
        // elimination and does not need to be pinned.
        let pinned_items: &[qsc_fir::fir::StoreItemId] = &[];
        let warnings = run_codegen_pipeline_to(
            package_store,
            &mut fir_store,
            fir_package_id,
            qsc_fir_transforms::PipelineStage::Full,
            pinned_items,
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
            backend,
        ))
    }

    /// Pin-based fallback for callable args containing non-lowerable closure captures.
    ///
    /// Seeds concrete (non-arrow-input) callables into the entry for reachability,
    /// pins arrow-input callables and the target for DCE survival, and lets
    /// `fir_to_qir_from_callable` handle specialization at QIR generation time.
    fn prepare_codegen_fir_from_callable_args_pinned(
        package_store: &PackageStore,
        callable: qsc_hir::hir::ItemId,
        args: &Value,
        capabilities: TargetCapabilityFlags,
        mut concrete_callables: FxHashSet<qsc_fir::fir::StoreItemId>,
    ) -> Result<CodegenFir, Vec<Error>> {
        let (mut fir_store, fir_package_id, _assigner) =
            lower_to_fir(package_store, callable.package, None, None);

        let target_callable = qsc_fir::fir::StoreItemId {
            package: qsc_lowerer::map_hir_package_to_fir(callable.package),
            item: qsc_lowerer::map_hir_local_item_to_fir(callable.item),
        };
        let callable_types = build_callable_type_map(&fir_store, &concrete_callables);
        validate_runtime_callable_args(&fir_store, target_callable, args, &callable_types)?;

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

        Ok(CodegenFir {
            fir_store,
            fir_package_id,
            compute_properties,
            warnings,
        })
    }

    /// Returns `true` if a value can be reconstructed inside the synthetic entry
    /// as FIR literals and callable references.
    ///
    /// Runtime identities such as allocated qubits and dynamic measurement results
    /// have no classical literal form and therefore cannot be lowered. Foreign
    /// closures also require reinvocation: FIR closure targets are package-local.
    fn value_is_fir_lowerable(value: &Value, package: qsc_fir::fir::PackageId) -> bool {
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
            Value::Tuple(vs, _) => vs
                .iter()
                .all(|value| value_is_fir_lowerable(value, package)),
            Value::Array(vs) => vs
                .iter()
                .all(|value| value_is_fir_lowerable(value, package)),
            Value::Closure(c) => {
                c.id.package == package
                    && c.fixed_args
                        .iter()
                        .all(|value| value_is_fir_lowerable(value, package))
            }
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
            lower_to_fir(package_store, package_id, package_override, None);

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
        let (mut fir_store, fir_package_id, _assigner) =
            lower_to_fir(package_store, callable.package, None, None);

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
