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
    /// - No arrow types or closures (defunctionalization complete)
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

    /// Pre-computed type information for a reachable global callable: its formal
    /// arrow type and the generic type parameters that type is quantified over.
    /// Cached so a call site can infer the callable's concrete generic arguments
    /// from the type expected at that site.
    #[derive(Clone)]
    struct CallableValueInfo {
        ty: qsc_fir::ty::Ty,
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

                let mut lowerer = qsc_lowerer::Lowerer::new();
                let fir_package = if id == package_id {
                    let mut fir_package = Package::default();
                    lowerer.lower_and_update_package(&mut fir_package, hir_package);
                    fir_package.entry_exec_graph = lowerer.take_exec_graph();
                    fir_package
                } else {
                    lowerer.lower_package(hir_package, &fir_store)
                };
                if id == package_id {
                    fir_assigner = lowerer.into_assigner();
                }
                fir_store.insert(qsc_lowerer::map_hir_package_to_fir(id), fir_package);
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
        let source_package = package_store
            .get(package_id)
            .expect("package should be in store");

        errors
            .into_iter()
            .map(|e| Error::Pass(WithSource::from_map(&source_package.sources, e)))
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
    ) -> (qsc_data_structures::span::Span, qsc_fir::ty::Ty) {
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

    fn seed_entry_with_callables(
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
            let normalized_ty = resolve_functor_params(&resolve_udt_ty(fir_store, &ty));
            map.insert(
                *id,
                CallableValueInfo {
                    ty: normalized_ty,
                    generics: callable_decl.generics.clone(),
                },
            );
        }
        map
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
        callables: &FxHashSet<qsc_fir::fir::StoreItemId>,
    ) {
        use qsc_fir::fir::{CallableImpl, Global, PackageLookup};

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
                        resolve_functor_params(&resolve_udt_ty(fir_store, &input_pat.ty)),
                        resolve_functor_params(&resolve_udt_ty(fir_store, &callable_decl.output)),
                    ))
                } else {
                    None
                };
                (*id, callable_decl.input, normalized_signature)
            })
            .collect();

        for (id, input_pat_id, normalized_signature) in normalized {
            let package = fir_store.get_mut(id.package);
            let qsc_fir::fir::ItemKind::Callable(callable_decl) = &mut package
                .items
                .get_mut(id.item)
                .expect("callable item should exist")
                .kind
            else {
                panic!("callable should exist in lowered package");
            };
            if let Some((input_ty, output_ty)) = normalized_signature {
                package
                    .pats
                    .get_mut(input_pat_id)
                    .expect("callable input pattern should exist")
                    .ty = input_ty;
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
                normalize_block_node_types(package, block_id);
            }
        }
    }

    fn normalize_block_node_types(
        package: &mut qsc_fir::fir::Package,
        block_id: qsc_fir::fir::BlockId,
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
                normalize_pat_node_types(package, pat_id);
            }
            if let Some(expr_id) = expr_id {
                normalize_expr_node_types(package, expr_id);
            }
        }

        let block = package
            .blocks
            .get_mut(block_id)
            .expect("callable block should exist");
        block.ty = resolve_functor_params(&block.ty);
    }

    fn normalize_pat_node_types(package: &mut qsc_fir::fir::Package, pat_id: qsc_fir::fir::PatId) {
        let child_pats = {
            let pat = package
                .pats
                .get_mut(pat_id)
                .expect("callable pattern should exist");
            pat.ty = resolve_functor_params(&pat.ty);
            match &pat.kind {
                qsc_fir::fir::PatKind::Tuple(pats) => pats.clone(),
                qsc_fir::fir::PatKind::Bind(_) | qsc_fir::fir::PatKind::Discard => Vec::new(),
            }
        };
        for child_pat in child_pats {
            normalize_pat_node_types(package, child_pat);
        }
    }

    fn normalize_expr_node_types(
        package: &mut qsc_fir::fir::Package,
        expr_id: qsc_fir::fir::ExprId,
    ) {
        let (child_exprs, child_blocks) = {
            let expr = package
                .exprs
                .get_mut(expr_id)
                .expect("callable expression should exist");
            expr.ty = resolve_functor_params(&expr.ty);
            child_nodes_for_expr_kind(&expr.kind)
        };

        for child_expr in child_exprs {
            normalize_expr_node_types(package, child_expr);
        }
        for child_block in child_blocks {
            normalize_block_node_types(package, child_block);
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
    /// Builds args matching the target callable's pure input type: callable-typed positions
    /// are filled with Var references to the concrete callables from the `args` Value;
    /// non-callable positions get typed placeholder literals (which are never evaluated —
    /// they exist only to make the Call structurally valid for defunctionalization).
    fn seed_entry_with_call_to_target(
        fir_store: &mut qsc_fir::fir::PackageStore,
        fir_package_id: qsc_fir::fir::PackageId,
        target_callable: qsc_fir::fir::StoreItemId,
        args: &Value,
        callable_types: &rustc_hash::FxHashMap<qsc_fir::fir::StoreItemId, CallableValueInfo>,
    ) {
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
            callable_decl.kind,
            callable_decl.functors,
            &formal_input_ty,
            &formal_output_ty,
            args,
            callable_types,
        );

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
        kind: qsc_fir::fir::CallableKind,
        functors: qsc_fir::ty::FunctorSetValue,
        formal_input_ty: &qsc_fir::ty::Ty,
        formal_output_ty: &qsc_fir::ty::Ty,
        args: &Value,
        callable_types: &rustc_hash::FxHashMap<qsc_fir::fir::StoreItemId, CallableValueInfo>,
    ) -> (
        Vec<qsc_fir::ty::GenericArg>,
        qsc_fir::ty::Ty,
        qsc_fir::ty::Ty,
        qsc_fir::ty::Ty,
    ) {
        let generic_args =
            infer_target_generic_args(generics, formal_input_ty, args, callable_types);
        let formal_arrow = qsc_fir::ty::Arrow {
            kind,
            input: Box::new(formal_input_ty.clone()),
            output: Box::new(formal_output_ty.clone()),
            functors: qsc_fir::ty::FunctorSet::Value(functors),
        };
        let instantiated_arrow =
            qsc_fir::ty::Scheme::new(generics.to_vec(), Box::new(formal_arrow.clone()))
                .instantiate(&generic_args)
                .unwrap_or(formal_arrow);
        let input_ty = resolve_functor_params(&instantiated_arrow.input);
        let output_ty = resolve_functor_params(&instantiated_arrow.output);
        let arrow_ty = qsc_fir::ty::Ty::Arrow(Box::new(qsc_fir::ty::Arrow {
            kind: instantiated_arrow.kind,
            input: Box::new(input_ty.clone()),
            output: Box::new(output_ty.clone()),
            functors: instantiated_arrow.functors,
        }));
        (generic_args, input_ty, output_ty, arrow_ty)
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
                        span: qsc_data_structures::span::Span::default(),
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
                                span: qsc_data_structures::span::Span::default(),
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
                        span: qsc_data_structures::span::Span::default(),
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
                span: qsc_data_structures::span::Span::default(),
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
    /// The lowerer may produce parametric functor sets for arrow-typed inputs. The synthetic
    /// Call uses concrete types to satisfy post-mono invariants without requiring actual
    /// monomorphization specialization of the pinned target.
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

    /// Builds concrete generic args from a target callable's input and the
    /// runtime argument values supplied to the synthetic entry.
    ///
    /// Any parameter that cannot be inferred from the argument value tree falls
    /// back to the same concrete defaults used before synthetic-entry inference:
    /// type parameters become `Unit`, and functor parameters become `Empty`.
    fn infer_target_generic_args(
        generics: &[qsc_fir::ty::TypeParameter],
        formal_input_ty: &qsc_fir::ty::Ty,
        args: &Value,
        callable_types: &rustc_hash::FxHashMap<qsc_fir::fir::StoreItemId, CallableValueInfo>,
    ) -> Vec<qsc_fir::ty::GenericArg> {
        let mut arg_map = rustc_hash::FxHashMap::default();
        if let Some(actual_input_ty) =
            value_ty_for_inference(args, Some(formal_input_ty), callable_types)
        {
            let _ = infer_generic_ty_args(formal_input_ty, &actual_input_ty, &mut arg_map);
        }

        generics
            .iter()
            .enumerate()
            .map(|(idx, param)| {
                let inferred = arg_map.get(&qsc_fir::ty::ParamId::from(idx));
                match (param, inferred) {
                    (
                        qsc_fir::ty::TypeParameter::Ty { .. },
                        Some(qsc_fir::ty::GenericArg::Ty(ty)),
                    ) if !ty_contains_param(ty) => qsc_fir::ty::GenericArg::Ty(ty.clone()),
                    (
                        qsc_fir::ty::TypeParameter::Functor(_),
                        Some(qsc_fir::ty::GenericArg::Functor(functors)),
                    ) if matches!(functors, qsc_fir::ty::FunctorSet::Value(_)) => {
                        qsc_fir::ty::GenericArg::Functor(*functors)
                    }
                    _ => default_generic_arg(param),
                }
            })
            .collect()
    }

    /// Produces the concrete fallback used when an individual generic parameter
    /// cannot be inferred from the runtime argument value.
    fn default_generic_arg(param: &qsc_fir::ty::TypeParameter) -> qsc_fir::ty::GenericArg {
        match param {
            qsc_fir::ty::TypeParameter::Functor(_) => qsc_fir::ty::GenericArg::Functor(
                qsc_fir::ty::FunctorSet::Value(qsc_fir::ty::FunctorSetValue::Empty),
            ),
            qsc_fir::ty::TypeParameter::Ty { .. } => {
                qsc_fir::ty::GenericArg::Ty(qsc_fir::ty::Ty::Tuple(Vec::new()))
            }
        }
    }

    /// Returns true when a type still contains an unresolved type parameter or
    /// parametric functor set.
    fn ty_contains_param(ty: &qsc_fir::ty::Ty) -> bool {
        match ty {
            qsc_fir::ty::Ty::Param(_) => true,
            qsc_fir::ty::Ty::Array(item) => ty_contains_param(item),
            qsc_fir::ty::Ty::Arrow(arrow) => {
                matches!(arrow.functors, qsc_fir::ty::FunctorSet::Param(_))
                    || ty_contains_param(&arrow.input)
                    || ty_contains_param(&arrow.output)
            }
            qsc_fir::ty::Ty::Tuple(items) => items.iter().any(ty_contains_param),
            qsc_fir::ty::Ty::Err
            | qsc_fir::ty::Ty::Infer(_)
            | qsc_fir::ty::Ty::Prim(_)
            | qsc_fir::ty::Ty::Udt(_) => false,
        }
    }

    /// Reconstructs the best FIR type shape available from an interpreter value.
    ///
    /// This is used only for generic inference. Runtime identities that cannot
    /// be lowered into synthetic FIR, such as qubits or dynamic variables, can
    /// still expose enough type information to instantiate the target arrow.
    /// Callable values prefer the expected type when it is compatible with the
    /// callable's declared generic scheme, preserving caller-provided concrete
    /// arrow types for generic globals.
    fn value_ty_for_inference(
        value: &Value,
        expected_ty: Option<&qsc_fir::ty::Ty>,
        callable_types: &rustc_hash::FxHashMap<qsc_fir::fir::StoreItemId, CallableValueInfo>,
    ) -> Option<qsc_fir::ty::Ty> {
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
            Value::Tuple(values, _) => {
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
                let item_ty = values
                    .first()
                    .and_then(|value| value_ty_for_inference(value, expected_item, callable_types))
                    .or_else(|| expected_item.cloned())?;
                Some(qsc_fir::ty::Ty::Array(Box::new(item_ty)))
            }
            Value::Global(id, _) => {
                let info = callable_types.get(id)?;
                if let Some(expected_ty) = expected_ty
                    && infer_global_generic_args(&info.generics, &info.ty, expected_ty).is_some()
                {
                    return Some(expected_ty.clone());
                }
                Some(info.ty.clone())
            }
            Value::Closure(closure) => {
                let info = callable_types.get(&closure.id)?;
                Some(partial_applied_closure_ty(
                    &info.ty,
                    closure.fixed_args.len(),
                ))
            }
            Value::Var(_) => expected_ty.cloned(),
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
                        pkg.exprs.insert(
                            id,
                            qsc_fir::fir::Expr {
                                id,
                                span: qsc_data_structures::span::Span::default(),
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
                span: qsc_data_structures::span::Span::default(),
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
            .expect("Global callable type must be pre-computed")
            .clone();
        let formal_ty = info.ty;
        let inferred_generic_args = expected_ty.and_then(|actual_ty| {
            infer_global_generic_args(&info.generics, &formal_ty, actual_ty)
                .map(|generic_args| (actual_ty.clone(), generic_args))
        });
        let (ty, generic_args) = inferred_generic_args.unwrap_or_else(|| (formal_ty, Vec::new()));
        let expr_id = assigner.next_expr();
        package.exprs.insert(
            expr_id,
            qsc_fir::fir::Expr {
                id: expr_id,
                span: qsc_data_structures::span::Span::default(),
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

    /// Infers the concrete generic arguments for a reference to a generic global
    /// callable by matching its formal type against the `actual_ty` expected at
    /// the use site.
    ///
    /// Returns `None` when the callable is non-generic or the two types do not
    /// unify; otherwise returns one `GenericArg` per declared parameter in
    /// declaration order.
    fn infer_global_generic_args(
        generics: &[qsc_fir::ty::TypeParameter],
        formal_ty: &qsc_fir::ty::Ty,
        actual_ty: &qsc_fir::ty::Ty,
    ) -> Option<Vec<qsc_fir::ty::GenericArg>> {
        if generics.is_empty() {
            return None;
        }
        let mut arg_map = rustc_hash::FxHashMap::default();
        if !infer_generic_ty_args(formal_ty, actual_ty, &mut arg_map) {
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
                    ) => Some(qsc_fir::ty::GenericArg::Ty(ty.clone())),
                    (
                        qsc_fir::ty::TypeParameter::Functor(_),
                        Some(qsc_fir::ty::GenericArg::Functor(functors)),
                    ) => Some(qsc_fir::ty::GenericArg::Functor(*functors)),
                    _ => None,
                },
            )
            .collect()
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
            current_id = wrap_expr_with_functor(
                package,
                assigner,
                current_id,
                ty,
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
                span: qsc_data_structures::span::Span::default(),
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
        let full_ty = callable_types
            .get(&closure.id)
            .expect("Closure callable type must be pre-computed")
            .ty
            .clone();

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
                    span: qsc_data_structures::span::Span::default(),
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
                span: qsc_data_structures::span::Span::default(),
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
        let span = qsc_data_structures::span::Span::default();
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
    /// args can be represented as FIR values, making the target and args
    /// entry-reachable for full pipeline participation. Falls back to a
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

        // Runtime identities (allocated qubits, dynamic values, and closures
        // that capture them) cannot be reconstructed as FIR literals, so they
        // keep the pin-based approach where partial evaluation supplies the
        // original values at QIR generation time. Fully lowerable values flow
        // into the self-contained synthetic entry below.
        if !value_is_fir_lowerable(args) {
            let codegen_fir = prepare_codegen_fir_from_callable_args_pinned(
                package_store,
                callable,
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

        let (mut fir_store, fir_package_id, _assigner) =
            lower_to_fir(package_store, callable.package, None);

        // Pre-compute callable value types before normalizing concrete callable
        // bodies, so closure values still expose the original generic target
        // signatures needed by monomorphization.
        let callable_types = build_callable_type_map(&fir_store, &concrete_callables);
        normalize_callable_signatures(&mut fir_store, &concrete_callables);

        // Build synthetic Call(Var(target), args) as the entry expression.
        // This makes the target and all callable args entry-reachable for pipeline transforms.
        seed_entry_with_call_to_target(
            &mut fir_store,
            fir_package_id,
            target_callable,
            args,
            &callable_types,
        );

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
        capabilities: TargetCapabilityFlags,
        mut concrete_callables: FxHashSet<qsc_fir::fir::StoreItemId>,
    ) -> Result<CodegenFir, Vec<Error>> {
        let (mut fir_store, fir_package_id, _assigner) =
            lower_to_fir(package_store, callable.package, None);

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

        let target_callable = qsc_fir::fir::StoreItemId {
            package: qsc_lowerer::map_hir_package_to_fir(callable.package),
            item: qsc_lowerer::map_hir_local_item_to_fir(callable.item),
        };

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
        let (mut fir_store, fir_package_id, _assigner) =
            lower_to_fir(package_store, callable.package, None);

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
            let source_package_id = match e.span() {
                Some(span) => span.package,
                None => package_id,
            };
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
        let (package_id, prepared_fir) = compile_to_codegen_fir(
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
            let source_package_id = match e.span() {
                Some(span) => span.package,
                None => package_id,
            };
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
        let (package_id, prepared_fir) = compile_to_codegen_fir(
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
            let source_package_id = match e.span() {
                Some(span) => span.package,
                None => package_id,
            };
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
