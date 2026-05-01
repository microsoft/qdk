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
        error::WithSource, language_features::LanguageFeatures, source::SourceMap,
        target::TargetCapabilityFlags,
    };
    use qsc_frontend::compile::{Dependencies, PackageStore};
    use qsc_partial_eval::{PartialEvalConfig, ProgramEntry};
    use qsc_passes::{PackageType, PassContext, run_fir_passes_for_callable};
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
        let errors =
            run_fir_passes_for_callable(fir_store, compute_properties, callable, capabilities);
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

    fn ty_contains_arrow(ty: &qsc_fir::ty::Ty) -> bool {
        match ty {
            qsc_fir::ty::Ty::Array(item) => ty_contains_arrow(item),
            qsc_fir::ty::Ty::Arrow(_) => true,
            qsc_fir::ty::Ty::Tuple(items) => items.iter().any(ty_contains_arrow),
            qsc_fir::ty::Ty::Infer(_)
            | qsc_fir::ty::Ty::Param(_)
            | qsc_fir::ty::Ty::Prim(_)
            | qsc_fir::ty::Ty::Udt(_)
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
            // Item removed by DCE or not a callable — treat as not having arrow input.
            return false;
        };

        ty_contains_arrow(&package.get_pat(callable_decl.input).ty)
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

        let mut assigner = qsc_fir::assigner::Assigner::new();
        if let Some(max_expr) = fir_store
            .get(fir_package_id)
            .exprs
            .iter()
            .map(|(id, _)| u32::from(id))
            .max()
        {
            assigner.set_next_expr(qsc_fir::fir::ExprId::from(max_expr + 1));
        }

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
    /// This is the key enabler for higher-order function and closure support in circuit/QIR generation.
    /// When a callable receives callable arguments (e.g., `ApplyOp(op: Qubit => Unit, q: Qubit)`),
    /// this function:
    ///
    /// 1. Collects all concrete (non-arrow-typed) callables from the argument values
    /// 2. Seeds concrete callables into the entry to make them reachable from the main entry point
    /// 3. Pins arrow-input callables to prevent their elimination during DCE
    /// 4. Runs the full pipeline with these constraints
    ///
    /// This allows specialization to occur, generating concrete code for each unique callable
    /// argument combination rather than leaving abstract higher-order code.
    pub fn prepare_codegen_fir_from_callable_args(
        package_store: &PackageStore,
        callable: qsc_hir::hir::ItemId,
        args: &Value,
        capabilities: TargetCapabilityFlags,
    ) -> Result<CodegenFir, Vec<Error>> {
        // Collect concrete callable args up-front to determine the code path.
        // This avoids relying on callable_has_arrow_input after the pipeline
        // has run, which can return different results after UDT erasure
        // transforms Ty::Udt into a tuple containing Ty::Arrow.
        let mut concrete_callables = FxHashSet::default();
        collect_concrete_qsharp_callables(args, &mut concrete_callables);

        if concrete_callables.is_empty() {
            // No callable args — standard preparation is sufficient.
            return prepare_codegen_fir_from_callable(package_store, callable, capabilities);
        }

        // Callable args found. Lower a fresh FIR store so we can seed
        // callables into the entry before the pipeline runs DCE.
        let (mut fir_store, fir_package_id, _assigner) =
            lower_to_fir(package_store, callable.package, None);

        // Separate callables into those with arrow inputs (need pinning for
        // DCE but cannot be seeded in the entry) and concrete ones (seeded).
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
        // Pin the target callable and any arrow-input callables referenced
        // by closure args so item DCE preserves them (and their transitive
        // dependencies) even though they are not seeded in the entry.
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
