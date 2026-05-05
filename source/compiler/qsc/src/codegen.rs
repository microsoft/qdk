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

    fn clone_fir_package(package: &Package) -> Package {
        Package {
            items: package.items.clone(),
            entry: package.entry,
            entry_exec_graph: package.entry_exec_graph.clone(),
            blocks: package.blocks.clone(),
            exprs: package.exprs.clone(),
            pats: package.pats.clone(),
            stmts: package.stmts.clone(),
        }
    }

    fn clone_fir_store(fir_store: &qsc_fir::fir::PackageStore) -> qsc_fir::fir::PackageStore {
        let mut cloned_store = qsc_fir::fir::PackageStore::new();
        for (package_id, package) in fir_store {
            cloned_store.insert(package_id, clone_fir_package(package));
        }
        cloned_store
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
                    let mut fir_package = Package {
                        items: Default::default(),
                        entry: None,
                        entry_exec_graph: Default::default(),
                        blocks: Default::default(),
                        exprs: Default::default(),
                        pats: Default::default(),
                        stmts: Default::default(),
                    };
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
        package_id: qsc_hir::hir::PackageId,
        fir_store: &mut qsc_fir::fir::PackageStore,
        fir_package_id: qsc_fir::fir::PackageId,
    ) -> Result<(), Vec<Error>> {
        run_codegen_pipeline_to(
            package_store,
            package_id,
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
        package_id: qsc_hir::hir::PackageId,
        fir_store: &mut qsc_fir::fir::PackageStore,
        fir_package_id: qsc_fir::fir::PackageId,
        stage: qsc_fir_transforms::PipelineStage,
        pinned_items: &[qsc_fir::fir::StoreItemId],
    ) -> Result<(), Vec<Error>> {
        // CONTRACT: On success, `run_pipeline_to` with `PipelineStage::Full` produces FIR
        // satisfying `InvariantLevel::PostAll`:
        //   - No `Ty::Param` in reachable code (monomorphization completed).
        //   - No `ExprKind::Return` in reachable code (return unification completed).
        //   - No `Ty::Arrow` params / `ExprKind::Closure` (defunctionalization completed).
        //   - No `Ty::Udt` / `ExprKind::Struct` / `Field::Path` (UDT erasure completed).
        //   - All exec-graph ranges populated (exec-graph rebuild completed).
        // Downstream codegen (QIR lowering, partial evaluation) assumes these invariants hold.
        // See `qsc_fir_transforms::invariants::check` for the authoritative checker.
        let pipeline_errors =
            qsc_fir_transforms::run_pipeline_to(fir_store, fir_package_id, stage, pinned_items);
        if !pipeline_errors.is_empty() {
            let source_package = package_store
                .get(package_id)
                .expect("package should be in store");
            return Err(pipeline_errors
                .into_iter()
                .map(|e| Error::FirTransform(WithSource::from_map(&source_package.sources, e)))
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
        assigner: &mut qsc_fir::assigner::Assigner,
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

        let entry_expr_id = assigner.next_expr();
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

    /// Builds a pre-computed map of callable types for all Global/Closure values in `args`.
    ///
    /// This allows `lower_value_to_expr` to look up arrow types without holding an immutable
    /// reference to the package store while also mutating a package.
    fn build_callable_type_map(
        fir_store: &qsc_fir::fir::PackageStore,
        callables: &FxHashSet<qsc_fir::fir::StoreItemId>,
    ) -> rustc_hash::FxHashMap<qsc_fir::fir::StoreItemId, qsc_fir::ty::Ty> {
        let mut map =
            rustc_hash::FxHashMap::with_capacity_and_hasher(callables.len(), Default::default());
        for id in callables {
            let (_, ty) = callable_expr_span_and_ty(fir_store, *id);
            map.insert(*id, ty);
        }
        map
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
        callable_types: &rustc_hash::FxHashMap<qsc_fir::fir::StoreItemId, qsc_fir::ty::Ty>,
    ) {
        use qsc_fir::fir::{Global, PackageLookup};

        // Pre-compute target's arrow type and input pattern type (immutable borrow of store).
        let package = fir_store.get(target_callable.package);
        let Some(Global::Callable(callable_decl)) = package.get_global(target_callable.item) else {
            panic!("target callable must exist in lowered package");
        };
        let span = callable_decl.span;
        let input_pat = package.get_pat(callable_decl.input);
        let input_ty = resolve_functor_params(&resolve_udt_ty(fir_store, &input_pat.ty));
        let output_ty = resolve_functor_params(&resolve_udt_ty(fir_store, &callable_decl.output));
        let arrow_ty = qsc_fir::ty::Ty::Arrow(Box::new(qsc_fir::ty::Arrow {
            kind: callable_decl.kind,
            input: Box::new(input_ty.clone()),
            output: Box::new(output_ty.clone()),
            functors: qsc_fir::ty::FunctorSet::Value(callable_decl.functors),
        }));

        // Build concrete generic args for the callee Var so monomorphization can
        // resolve FunctorSet::Param in the specialized clone's body types.
        let generic_args = build_concrete_generic_args(&callable_decl.generics);

        // Build assigner from the package's current ID counters.
        let mut assigner = qsc_fir::assigner::Assigner::from_package(fir_store.get(fir_package_id));

        // Get the package mutably and build args expression matching the input type.
        let package = fir_store.get_mut(fir_package_id);
        let args_expr_id =
            build_synthetic_args(package, &mut assigner, &input_ty, args, callable_types);

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
                ty: output_ty,
                kind: qsc_fir::fir::ExprKind::Call(callee_expr_id, args_expr_id),
                exec_graph_range: qsc_fir::fir::ExecGraphIdx::ZERO
                    ..qsc_fir::fir::ExecGraphIdx::ZERO,
            },
        );

        // Set entry to the synthetic Call.
        package.entry = Some(call_expr_id);
        package.entry_exec_graph = Default::default();
    }

    /// Builds an args expression matching the target's input type.
    ///
    /// For callable-typed positions, uses the corresponding callable from `args`.
    /// For non-callable positions, uses `lower_value_to_expr` if the value is available
    /// in `args`, otherwise creates a typed placeholder literal.
    fn build_synthetic_args(
        package: &mut qsc_fir::fir::Package,
        assigner: &mut qsc_fir::assigner::Assigner,
        input_ty: &qsc_fir::ty::Ty,
        args: &Value,
        callable_types: &rustc_hash::FxHashMap<qsc_fir::fir::StoreItemId, qsc_fir::ty::Ty>,
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
                                    callable_types,
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
                lower_value_to_expr(package, assigner, args, callable_types)
            }
            _ => {
                // Non-callable position — lower value if possible, otherwise placeholder.
                match args {
                    Value::Qubit(_) | Value::Var(_) => {
                        make_placeholder_expr(package, assigner, input_ty)
                    }
                    _ => lower_value_to_expr(package, assigner, args, callable_types),
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

    /// Builds concrete generic args from a callable's generic parameter list.
    ///
    /// For each `TypeParameter::Functor`, produces `GenericArg::Functor(Value(Empty))`.
    /// For each `TypeParameter::Ty`, produces `GenericArg::Ty(Tuple([]))` (unit).
    /// These concrete args let monomorphization create a fully resolved specialization.
    fn build_concrete_generic_args(
        generics: &[qsc_fir::ty::TypeParameter],
    ) -> Vec<qsc_fir::ty::GenericArg> {
        generics
            .iter()
            .map(|param| match param {
                qsc_fir::ty::TypeParameter::Functor(_) => qsc_fir::ty::GenericArg::Functor(
                    qsc_fir::ty::FunctorSet::Value(qsc_fir::ty::FunctorSetValue::Empty),
                ),
                qsc_fir::ty::TypeParameter::Ty { .. } => {
                    qsc_fir::ty::GenericArg::Ty(qsc_fir::ty::Ty::Tuple(Vec::new()))
                }
            })
            .collect()
    }

    /// Extracts the specialized target callable from the entry Call expression after pipeline.
    ///
    /// After defunctionalization, the entry Call's callee Var references the specialized
    /// (post-defunc) version of the target callable. This function extracts that ID.
    #[allow(dead_code)]
    fn extract_target_from_entry_call(
        fir_store: &qsc_fir::fir::PackageStore,
        fir_package_id: qsc_fir::fir::PackageId,
    ) -> qsc_fir::fir::StoreItemId {
        let package = fir_store.get(fir_package_id);
        let entry_id = package
            .entry
            .expect("package must have entry after pipeline");
        let entry_expr = package.exprs.get(entry_id).expect("entry expr must exist");

        let qsc_fir::fir::ExprKind::Call(callee_id, _) = &entry_expr.kind else {
            panic!(
                "entry expression must be a Call after pipeline, found {:?}",
                entry_expr.kind
            );
        };

        let callee_expr = package
            .exprs
            .get(*callee_id)
            .expect("callee expr must exist");
        let qsc_fir::fir::ExprKind::Var(qsc_fir::fir::Res::Item(item_id), _) = &callee_expr.kind
        else {
            panic!(
                "entry Call callee must be a Var(Res::Item(...)) after pipeline, found {:?}",
                callee_expr.kind
            );
        };

        qsc_fir::fir::StoreItemId {
            package: item_id.package,
            item: item_id.item,
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
        callable_types: &rustc_hash::FxHashMap<qsc_fir::fir::StoreItemId, qsc_fir::ty::Ty>,
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
                let mut lowered_ids = Vec::with_capacity(vs.len());
                let mut lowered_tys = Vec::with_capacity(vs.len());
                for v in vs.iter() {
                    let id = lower_value_to_expr(package, assigner, v, callable_types);
                    lowered_tys.push(package.exprs.get(id).expect("just inserted").ty.clone());
                    lowered_ids.push(id);
                }
                (
                    qsc_fir::fir::ExprKind::Tuple(lowered_ids),
                    qsc_fir::ty::Ty::Tuple(lowered_tys),
                )
            }
            Value::Array(vs) => {
                let mut lowered_ids = Vec::with_capacity(vs.len());
                for v in vs.iter() {
                    lowered_ids.push(lower_value_to_expr(package, assigner, v, callable_types));
                }
                let elem_ty = lowered_ids.first().map_or(qsc_fir::ty::Ty::Err, |id| {
                    package.exprs.get(*id).expect("just inserted").ty.clone()
                });
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
                return lower_global_to_expr(package, assigner, *id, *functor, callable_types);
            }
            Value::Closure(c) => {
                return lower_closure_to_expr(package, assigner, c, callable_types);
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
        callable_types: &rustc_hash::FxHashMap<qsc_fir::fir::StoreItemId, qsc_fir::ty::Ty>,
    ) -> qsc_fir::fir::ExprId {
        let ty = callable_types
            .get(&id)
            .expect("Global callable type must be pre-computed")
            .clone();
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
                    Vec::new(),
                ),
                exec_graph_range: qsc_fir::fir::ExecGraphIdx::ZERO
                    ..qsc_fir::fir::ExecGraphIdx::ZERO,
            },
        );
        wrap_expr_with_functor_app(package, assigner, expr_id, &ty, functor)
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

    /// Lowers a captureless closure to its underlying callable variable expression.
    ///
    /// Capturing closures take the pinned fallback path before this is called, so
    /// this helper only has to preserve the closure target and runtime functor app.
    fn lower_closure_to_expr(
        package: &mut qsc_fir::fir::Package,
        assigner: &mut qsc_fir::assigner::Assigner,
        closure: &qsc_eval::val::Closure,
        callable_types: &rustc_hash::FxHashMap<qsc_fir::fir::StoreItemId, qsc_fir::ty::Ty>,
    ) -> qsc_fir::fir::ExprId {
        // For the synthetic entry, we emit a Var referencing the closure's underlying
        // callable. Captures are irrelevant for pipeline reachability — defunc handles
        // specialization. Both captureless and capturing closures use the same Var form.
        let ty = callable_types
            .get(&closure.id)
            .expect("Closure callable type must be pre-computed")
            .clone();
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
                ty: ty.clone(),
                kind,
                exec_graph_range: qsc_fir::fir::ExecGraphIdx::ZERO
                    ..qsc_fir::fir::ExecGraphIdx::ZERO,
            },
        );
        wrap_expr_with_functor_app(package, assigner, expr_id, &ty, closure.functor)
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
    /// Uses a synthetic `Call(Var(target), args)` entry expression when callable args
    /// can be represented as FIR values, making the target and args entry-reachable for full
    /// pipeline participation. Falls back to a pin-based approach when:
    /// - Args contain closures with captures (partial applications require capture context
    ///   that can't be represented in the synthetic Call)
    ///
    /// The original target is pinned for DCE survival so that `fir_to_qir_from_callable`
    /// can still use the original ID for partial evaluation.
    pub fn prepare_codegen_fir_from_callable_args(
        package_store: &PackageStore,
        callable: qsc_hir::hir::ItemId,
        args: &Value,
        capabilities: TargetCapabilityFlags,
    ) -> Result<CodegenFir, Vec<Error>> {
        let mut concrete_callables = FxHashSet::default();
        collect_concrete_qsharp_callables(args, &mut concrete_callables);

        if concrete_callables.is_empty() {
            return prepare_codegen_fir_from_callable(package_store, callable, capabilities);
        }

        // Closures with captures represent partial applications whose capture context
        // can't be lowered into a synthetic Call expression yet. They still use the
        // pin-based approach where partial eval handles specialization at QIR generation time.
        if has_closure_with_captures(args) {
            return prepare_codegen_fir_from_callable_args_pinned(
                package_store,
                callable,
                args,
                capabilities,
                concrete_callables,
            );
        }

        let (mut fir_store, fir_package_id, _assigner) =
            lower_to_fir(package_store, callable.package, None);

        let target_callable = qsc_fir::fir::StoreItemId {
            package: qsc_lowerer::map_hir_package_to_fir(callable.package),
            item: qsc_lowerer::map_hir_local_item_to_fir(callable.item),
        };

        // Pre-compute callable type map (immutable store access) before mutating.
        let callable_types = build_callable_type_map(&fir_store, &concrete_callables);

        // Build synthetic Call(Var(target), args) as the entry expression.
        // This makes the target and all callable args entry-reachable for pipeline transforms.
        seed_entry_with_call_to_target(
            &mut fir_store,
            fir_package_id,
            target_callable,
            args,
            &callable_types,
        );

        // Pin the original target for DCE survival. After defunc rewrites the entry
        // Call callee to reference the specialized version, the original target becomes
        // unreachable. Pinning keeps it alive for `fir_to_qir_from_callable` which
        // uses the original ID with original-shaped args.
        run_codegen_pipeline_to(
            package_store,
            callable.package,
            &mut fir_store,
            fir_package_id,
            qsc_fir_transforms::PipelineStage::Full,
            &[target_callable],
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
        })
    }

    /// Pin-based fallback for callable args containing closures with captures.
    ///
    /// Seeds concrete (non-arrow-input) callables into the entry for reachability,
    /// pins arrow-input callables and the target for DCE survival, and lets
    /// `fir_to_qir_from_callable` handle specialization at QIR generation time.
    fn prepare_codegen_fir_from_callable_args_pinned(
        package_store: &PackageStore,
        callable: qsc_hir::hir::ItemId,
        _args: &Value,
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
        run_codegen_pipeline_to(
            package_store,
            callable.package,
            &mut fir_store,
            fir_package_id,
            qsc_fir_transforms::PipelineStage::Full,
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
        })
    }

    /// Returns `true` if the value tree contains any closures with captures.
    fn has_closure_with_captures(value: &Value) -> bool {
        match value {
            Value::Closure(c) => !c.fixed_args.is_empty(),
            Value::Tuple(vs, _) => vs.iter().any(has_closure_with_captures),
            Value::Array(vs) => vs.iter().any(has_closure_with_captures),
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
        run_codegen_pipeline(package_store, package_id, &mut fir_store, fir_package_id)?;

        let compute_properties =
            PassContext::run_fir_passes_on_fir(&fir_store, fir_package_id, capabilities)
                .map_err(|errors| map_pass_errors(package_store, package_id, errors))?;

        Ok(CodegenFir {
            fir_store,
            fir_package_id,
            compute_properties,
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
            clone_fir_store(fir_store),
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
        let (mut fir_store, fir_package_id, mut assigner) =
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
            });
        }

        seed_entry_with_callable(&mut fir_store, fir_package_id, callable, &mut assigner);
        run_codegen_pipeline(
            package_store,
            callable.package,
            &mut fir_store,
            fir_package_id,
        )?;

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
