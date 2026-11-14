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
    use qsc_eval::callable::{TypeArgs, TypeArgsError, TypeBudget};
    use qsc_eval::val::Value;
    use qsc_fir::fir::Package;
    use qsc_fir::ty::{FunctorSet, GenericArg, ParamId, Ty};

    use qsc_data_structures::{
        error::WithSource,
        functors::FunctorApp,
        language_features::LanguageFeatures,
        source::SourceMap,
        target::{Profile, TargetCapabilityFlags},
    };
    use qsc_frontend::compile::{Dependencies, PackageStore};
    use qsc_partial_eval::{PartialEvalConfig, ProgramEntry};
    use qsc_passes::{PackageType, PassContext, run_rca_for_callable};
    use rustc_hash::{FxHashMap, FxHashSet};

    use crate::interpret::Error;

    /// Successful generation, including nonfatal source-located diagnostics.
    /// A recovered full-Adaptive request emits `Adaptive_RIF` / QIR 1.0.
    #[derive(Debug)]
    pub struct QirGenerationResult {
        pub qir: String,
        pub capabilities: TargetCapabilityFlags,
        pub warnings: Vec<Error>,
    }

    /// Raw and transformed RIR for the selected backend, with nonfatal diagnostics.
    pub struct RirGenerationResult {
        pub rir: Vec<String>,
        pub capabilities: TargetCapabilityFlags,
        pub warnings: Vec<Error>,
    }

    #[derive(Clone, Debug, miette::Diagnostic, thiserror::Error)]
    #[error(
        "requested Adaptive used Adaptive_RIF fallback after RCA capability rejection ({reason}); effective output is QIR 1.0"
    )]
    #[diagnostic(
        code("Qdk.Qsc.Codegen.AdaptiveProfileFallback"),
        severity(warning),
        forward(reason)
    )]
    pub struct AdaptiveProfileFallback {
        pub reason: Box<Error>,
    }

    impl CodegenFir {
        fn take_generation_warnings(&mut self) -> Vec<Error> {
            if let Some(reason) = self.fallback_reason.take() {
                self.warnings
                    .push(Error::AdaptiveProfileFallback(AdaptiveProfileFallback {
                        reason: Box::new(reason),
                    }));
            }
            std::mem::take(&mut self.warnings)
        }

        pub(crate) fn generated(mut self, qir: String) -> QirGenerationResult {
            let warnings = self.take_generation_warnings();
            QirGenerationResult {
                qir,
                capabilities: self.capabilities,
                warnings,
            }
        }

        pub(crate) fn generated_rir(mut self, rir: Vec<String>) -> RirGenerationResult {
            let warnings = self.take_generation_warnings();
            RirGenerationResult {
                rir,
                capabilities: self.capabilities,
                warnings,
            }
        }
    }

    /// Profile-specific analysis of the unchanged transformed program.
    pub struct QirAnalysis {
        pub compute_properties: qsc_rca::PackageStoreComputeProperties,
        pub capabilities: TargetCapabilityFlags,
        pub fallback_reason: Option<qsc_passes::Error>,
    }

    /// Allows one `Adaptive_RIF` attempt after any nonempty full-Adaptive RCA rejection.
    ///
    /// Error kinds do not determine whether the narrower backend can compile the
    /// program. The retry must independently pass capability analysis and codegen.
    #[must_use]
    pub fn adaptive_fallback_eligible(
        capabilities: TargetCapabilityFlags,
        errors: &[qsc_passes::Error],
    ) -> bool {
        capabilities == Profile::Adaptive.into() && !errors.is_empty()
    }

    /// Analyzes FIR runtime capabilities and validates support, with bounded
    /// fallback from full Adaptive to `Adaptive_RIF`.
    ///
    /// Analyzes the store under the requested capabilities, then checks the
    /// supplied callable, or `package_id` when no callable is supplied. If
    /// [`adaptive_fallback_eligible`] accepts the errors, repeats that analysis
    /// and validation once under `Adaptive_RIF` on the same unchanged FIR.
    ///
    /// Returns the successful analysis, its effective capabilities, and the
    /// original rejection that prompted fallback, if any. This function neither
    /// generates IR nor publishes warnings; callers report fallback only after
    /// generation succeeds, not when merely checking support.
    ///
    /// # Errors
    /// Returns the original capability errors when fallback is ineligible.
    /// If fallback also fails, retains the original diagnostics and adds one
    /// failed-fallback note to the first diagnostic's help.
    pub fn analyze_capabilities_with_fallback(
        fir_store: &qsc_fir::fir::PackageStore,
        package_id: qsc_fir::fir::PackageId,
        callable: Option<qsc_fir::fir::StoreItemId>,
        capabilities: TargetCapabilityFlags,
    ) -> Result<QirAnalysis, Vec<qsc_passes::Error>> {
        let analyze = |capabilities| {
            if let Some(callable) = callable {
                let properties = qsc_rca::Analyzer::init(fir_store, capabilities).analyze_all();
                let errors = run_rca_for_callable(fir_store, &properties, callable, capabilities);
                if errors.is_empty() {
                    Ok(properties)
                } else {
                    Err(errors)
                }
            } else {
                PassContext::run_fir_passes_on_fir(fir_store, package_id, capabilities)
            }
        };
        run_rca_with_adaptive_fallback(capabilities, analyze)
    }

    /// Runs runtime capability analysis (RCA) with at most one Adaptive-to-`Adaptive_RIF` retry.
    ///
    /// Calls `analyze` with `capabilities` first. Any nonempty RCA rejection under
    /// full Adaptive triggers a second call with `Adaptive_RIF`, regardless of
    /// the individual error kinds. The callback must perform only capability
    /// analysis and validation on the same unchanged FIR, computing fresh
    /// properties for each invocation's capabilities.
    ///
    /// Returns the successful compute properties and effective capabilities.
    /// After successful fallback, `fallback_reason` contains the first error
    /// from the original attempt; otherwise it is `None`. This function neither
    /// generates IR nor publishes diagnostics.
    ///
    /// # Errors
    /// Returns the original errors unchanged when fallback is ineligible.
    /// If fallback also fails, preserves the original errors' messages, codes,
    /// spans, and order, adding one failed-fallback note to the first error's help.
    pub(super) fn run_rca_with_adaptive_fallback(
        capabilities: TargetCapabilityFlags,
        mut analyze: impl FnMut(
            TargetCapabilityFlags,
        ) -> Result<
            qsc_rca::PackageStoreComputeProperties,
            Vec<qsc_passes::Error>,
        >,
    ) -> Result<QirAnalysis, Vec<qsc_passes::Error>> {
        match analyze(capabilities) {
            Ok(compute_properties) => Ok(QirAnalysis {
                compute_properties,
                capabilities,
                fallback_reason: None,
            }),
            Err(mut errors) if adaptive_fallback_eligible(capabilities, &errors) => {
                let effective: TargetCapabilityFlags = Profile::AdaptiveRIF.into();
                if let Ok(compute_properties) = analyze(effective) {
                    Ok(QirAnalysis {
                        compute_properties,
                        capabilities: effective,
                        fallback_reason: Some(errors.remove(0)),
                    })
                } else {
                    let original = errors.remove(0);
                    errors.insert(
                        0,
                        qsc_passes::Error::AdaptiveFallbackProof(
                            qsc_passes::AdaptiveFallbackAnalysisError::new(original),
                        ),
                    );
                    Err(errors)
                }
            }
            Err(errors) => Err(errors),
        }
    }

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
        pub capabilities: TargetCapabilityFlags,
        pub fallback_reason: Option<Error>,
    }

    fn prepare_analysis(
        sources: &PackageStore,
        store: &qsc_fir::fir::PackageStore,
        package: qsc_fir::fir::PackageId,
        callable: Option<qsc_fir::fir::StoreItemId>,
        capabilities: TargetCapabilityFlags,
        allow_fallback: bool,
    ) -> Result<QirAnalysis, Vec<Error>> {
        let result = if allow_fallback {
            analyze_capabilities_with_fallback(store, package, callable, capabilities)
        } else if let Some(callable) = callable {
            let properties = qsc_rca::Analyzer::init(store, capabilities).analyze_all();
            let errors = run_rca_for_callable(store, &properties, callable, capabilities);
            if errors.is_empty() {
                Ok(QirAnalysis {
                    compute_properties: properties,
                    capabilities,
                    fallback_reason: None,
                })
            } else {
                Err(errors)
            }
        } else {
            PassContext::run_fir_passes_on_fir(store, package, capabilities).map(
                |compute_properties| QirAnalysis {
                    compute_properties,
                    capabilities,
                    fallback_reason: None,
                },
            )
        };
        result.map_err(|errors| {
            map_pass_errors(
                sources,
                qsc_lowerer::map_fir_package_to_hir(package),
                errors,
            )
        })
    }

    /// Dispatch signal indicating which QIR-generation route a consumer of
    /// [`prepare_codegen_fir_from_callable_args`] must take.
    ///
    /// - `SyntheticEntry`: the prepared FIR carries a self-contained synthetic
    ///   entry expression; build QIR via `entry_from_codegen_fir` + `fir_to_qir`.
    /// - `ReinvokeOriginal`: invoke the returned target and argument values through
    ///   `fir_to_qir_from_callable`. The target may be a private specialization;
    ///   runtime payload identities are preserved instead of materialized as FIR.
    pub enum CallableArgsBackend {
        SyntheticEntry,
        ReinvokeOriginal {
            callable: qsc_fir::fir::StoreItemId,
            args: Value,
        },
    }

    /// Concrete signature for a runtime callable.
    #[derive(Clone)]
    struct CallableValueInfo {
        /// Nominal types remain intact until the normal UDT-erasure pass.
        ty: qsc_fir::ty::Ty,
    }

    struct RuntimeCallableTypes {
        callables: FxHashMap<qsc_fir::fir::StoreItemId, CallableValueInfo>,
        udts: FxHashMap<qsc_fir::fir::StoreItemId, Ty>,
    }

    struct RuntimeSpecialization {
        source: qsc_fir::fir::StoreItemId,
        arguments: Vec<(ParamId, GenericArg)>,
        target: qsc_fir::fir::StoreItemId,
    }

    /// Resolves runtime callable instances in a private codegen store without
    /// changing interpreter values or source declarations.
    ///
    /// Caches parameter discovery by source identity and concrete targets by
    /// source identity plus resolved arguments. Specializations consume the
    /// same budget as the subsequent monomorphization pipeline.
    struct RuntimeInstantiator<'a> {
        sources: &'a PackageStore,
        store: &'a mut qsc_fir::fir::PackageStore,
        budget: &'a mut qsc_fir_transforms::SpecializationBudget,
        parameters: FxHashMap<qsc_fir::fir::StoreItemId, Vec<ParamId>>,
        specializations: Vec<RuntimeSpecialization>,
    }

    impl RuntimeInstantiator<'_> {
        /// Returns a target whose required type and functor parameters are concrete.
        ///
        /// Keeps `id` when no parameters are required; otherwise resolves the
        /// recorded arguments and creates or reuses a specialized copy. An unbound
        /// declaration can supply implicit functor bounds, but type arguments are
        /// never inferred from runtime values. Generic callees in the copied body
        /// remain the responsibility of the normal monomorphization pipeline.
        ///
        /// # Errors
        /// Returns source-located diagnostics for missing, unresolved, or wrong-kind
        /// bindings, or exceeded specialization/type-expansion limits. The caller
        /// must discard the private codegen store after an error.
        ///
        /// # Panics
        /// Panics if `id` does not identify a callable in the FIR store.
        fn instantiate_target(
            &mut self,
            id: qsc_fir::fir::StoreItemId,
            type_args: &TypeArgs,
        ) -> Result<qsc_fir::fir::StoreItemId, Vec<Error>> {
            use qsc_fir::fir::{Global, PackageLookup};

            let package = self.store.get(id.package);
            let Some(Global::Callable(decl)) = package.get_global(id.item) else {
                panic!("runtime target must exist");
            };
            let span = decl.name.span;
            let parameters = self.parameters.entry(id).or_insert_with(|| {
                let mut parameters: Vec<_> =
                    qsc_fir_transforms::runtime_callable_parameters(package, id.item)
                        .into_iter()
                        .collect();
                parameters.sort_by_key(ToString::to_string);
                parameters
            });
            if parameters.is_empty() {
                return Ok(id);
            }
            let native_args;
            let type_args = if matches!(type_args, TypeArgs::Unbound) {
                native_args = TypeArgs::from_decl(id, &decl.generics);
                &native_args
            } else {
                type_args
            };
            let mut type_budget = TypeBudget::default();
            let arguments = parameters
                .iter()
                .map(|&param| {
                    type_args
                        .resolve_argument(param, &mut type_budget)
                        .map(|arg| (param, arg))
                })
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| self.argument_error(span, &error))?;
            if let Some(existing) = self
                .specializations
                .iter()
                .find(|existing| existing.source == id && existing.arguments == arguments)
            {
                return Ok(existing.target);
            }
            let item = qsc_fir_transforms::instantiate_runtime_closure(
                self.store.get_mut(id.package),
                id.item,
                arguments.clone(),
                self.budget,
            )
            .map_err(|error| {
                vec![Error::FirTransform(
                    crate::compile::attach_fir_transform_source(self.sources, error),
                )]
            })?
            .ok_or_else(|| self.argument_error(span, &TypeArgsError::Unresolved))?;
            let target = qsc_fir::fir::StoreItemId {
                package: id.package,
                item,
            };
            self.specializations.push(RuntimeSpecialization {
                source: id,
                arguments,
                target,
            });
            Ok(target)
        }

        /// Maps argument-resolution failures to diagnostics at the supplied source span.
        ///
        /// Expansion failures retain the monomorphization limit diagnostic; other
        /// resolution failures become the entry-point missing-type-arguments diagnostic.
        fn argument_error(
            &self,
            span: qsc_fir::fir::PackageSpan,
            error: &TypeArgsError,
        ) -> Vec<Error> {
            if matches!(error, TypeArgsError::ExpansionLimit) {
                vec![Error::FirTransform(
                    crate::compile::attach_fir_transform_source(
                        self.sources,
                        qsc_fir_transforms::MonomorphizationError::TypeExpansionLimit(span).into(),
                    ),
                )]
            } else {
                map_pass_errors(
                    self.sources,
                    qsc_lowerer::map_fir_package_to_hir(span.package),
                    vec![qsc_passes::Error::EntryPoint(
                        qsc_passes::EntryPointError::MissingTypeArguments(span.span),
                    )],
                )
            }
        }

        /// Rebuilds a value tree, redirecting its callables through [`Self::instantiate_target`].
        ///
        /// Arrays, tuples, and closure captures are traversed recursively. Callable
        /// functors, nominal tuple identities, and captured data are preserved;
        /// consumed callable type arguments are replaced with `TypeArgs::Empty`.
        ///
        /// Unlike [`Clone::clone`], this rebuilds aggregate storage even when a
        /// subtree contains no callables. Other values use ordinary cloning,
        /// preserving runtime qubit and dynamic-value identities. The input value
        /// is unchanged, but target instantiation may add private FIR implementations.
        ///
        /// # Errors
        /// Propagates instantiation diagnostics from any nested callable.
        ///
        /// # Panics
        /// Panics if a referenced target does not identify a callable in the FIR store.
        fn instantiate_value(&mut self, value: &Value) -> Result<Value, Vec<Error>> {
            Ok(match value {
                Value::Global(..) | Value::GlobalWithArgs(..) => {
                    let global = value.global_instance().expect("global callable");
                    Value::global_with_args(
                        self.instantiate_target(global.id, &global.type_args)?,
                        global.functor,
                        TypeArgs::Empty,
                    )
                }
                Value::Closure(closure) => {
                    let id = self.instantiate_target(closure.id, &closure.type_args)?;
                    let fixed_args = closure
                        .fixed_args
                        .iter()
                        .map(|value| self.instantiate_value(value))
                        .collect::<Result<Vec<_>, _>>()?;
                    Value::Closure(Box::new(qsc_eval::val::Closure {
                        id,
                        fixed_args: fixed_args.into(),
                        functor: closure.functor,
                        type_args: TypeArgs::Empty,
                    }))
                }
                Value::Array(values) => Value::Array(
                    values
                        .iter()
                        .map(|value| self.instantiate_value(value))
                        .collect::<Result<Vec<_>, _>>()?
                        .into(),
                ),
                Value::Tuple(values, nominal) => Value::Tuple(
                    values
                        .iter()
                        .map(|value| self.instantiate_value(value))
                        .collect::<Result<Vec<_>, _>>()?
                        .into(),
                    nominal.clone(),
                ),
                other => other.clone(),
            })
        }
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
        run_codegen_pipeline_with_budget(
            package_store,
            fir_store,
            fir_package_id,
            stage,
            pinned_items,
            &mut qsc_fir_transforms::SpecializationBudget::default(),
        )
    }

    fn run_codegen_pipeline_with_budget(
        package_store: &PackageStore,
        fir_store: &mut qsc_fir::fir::PackageStore,
        fir_package_id: qsc_fir::fir::PackageId,
        stage: qsc_fir_transforms::PipelineStage,
        pinned_items: &[qsc_fir::fir::StoreItemId],
        budget: &mut qsc_fir_transforms::SpecializationBudget,
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
        let pipeline_result = qsc_fir_transforms::run_pipeline_to_with_specialization_budget(
            fir_store,
            fir_package_id,
            stage,
            pinned_items,
            budget,
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
    ) -> Result<Vec<Error>, Vec<Error>> {
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

    pub(super) fn map_pass_errors(
        package_store: &PackageStore,
        package_id: qsc_hir::hir::PackageId,
        errors: Vec<qsc_passes::Error>,
    ) -> Vec<Error> {
        errors
            .into_iter()
            .map(|e| {
                // A capability error can point into a dependency, so resolve it
                // against the package that actually owns its span.
                let owner = e.capability_error().map_or(package_id, |inner| {
                    qsc_lowerer::map_fir_package_to_hir(inner.span().package)
                });
                let source_package = package_store
                    .get(owner)
                    .expect("package should be in store");
                Error::Pass(WithSource::from_map(&source_package.sources, e))
            })
            .collect()
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

    pub(crate) fn seed_entry_with_callables(
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
    /// Runtime instantiation has already made every supplied callable concrete.
    fn build_callable_type_map(
        fir_store: &qsc_fir::fir::PackageStore,
        callables: &FxHashSet<qsc_fir::fir::StoreItemId>,
    ) -> RuntimeCallableTypes {
        use qsc_fir::fir::{Global, PackageLookup};

        let mut map =
            rustc_hash::FxHashMap::with_capacity_and_hasher(callables.len(), Default::default());
        for id in callables {
            let package = fir_store.get(id.package);
            let Some(Global::Callable(_)) = package.get_global(id.item) else {
                panic!("callable should exist in lowered package");
            };
            let (_, ty) = callable_expr_span_and_ty(fir_store, *id);
            map.insert(*id, CallableValueInfo { ty });
        }
        let udts = fir_store
            .iter()
            .flat_map(|(package_id, package)| {
                package.items.iter().filter_map(move |(id, item)| {
                    if let qsc_fir::fir::ItemKind::Ty(_, udt) = &item.kind {
                        Some(((package_id, id).into(), udt.get_pure_ty()))
                    } else {
                        None
                    }
                })
            })
            .collect();
        RuntimeCallableTypes {
            callables: map,
            udts,
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
    /// Materializes the validated argument values with their declared nominal types.
    /// Captures become local bindings evaluated before the call. The resulting entry
    /// is self-contained and is executed directly, not followed by a second invocation
    /// of the original callable.
    #[allow(clippy::too_many_lines)]
    fn seed_entry_with_call_to_target(
        fir_store: &mut qsc_fir::fir::PackageStore,
        fir_package_id: qsc_fir::fir::PackageId,
        target_callable: qsc_fir::fir::StoreItemId,
        args: &Value,
        callable_types: &RuntimeCallableTypes,
    ) {
        use qsc_fir::fir::{Global, PackageLookup};

        // Pre-compute target's arrow type and input pattern type (immutable borrow of store).
        let package = fir_store.get(target_callable.package);
        let Some(Global::Callable(callable_decl)) = package.get_global(target_callable.item) else {
            panic!("target callable must exist in lowered package");
        };
        let span = callable_decl.span;
        let input_pat = package.get_pat(callable_decl.input);
        let formal_input_ty = input_pat.ty.clone();
        let formal_output_ty = callable_decl.output.clone();
        let input_ty = formal_input_ty;
        let output_ty = formal_output_ty;
        let arrow_ty = Ty::Arrow(Box::new(qsc_fir::ty::Arrow {
            kind: callable_decl.kind,
            input: Box::new(input_ty.clone()),
            output: Box::new(output_ty.clone()),
            functors: FunctorSet::Value(callable_decl.functors),
        }));

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
                    Vec::new(),
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

    /// Lowers checked arguments with their declared tuple shape and element types.
    ///
    /// Non-lowerable runtime identities must use the pinned preparation route.
    ///
    /// # Panics
    /// Panics if a tuple argument does not match its checked shape.
    fn build_synthetic_args(
        package: &mut qsc_fir::fir::Package,
        assigner: &mut qsc_fir::assigner::Assigner,
        input_ty: &qsc_fir::ty::Ty,
        args: &Value,
        callable_types: &RuntimeCallableTypes,
        pending_stmts: &mut Vec<qsc_fir::fir::StmtId>,
    ) -> qsc_fir::fir::ExprId {
        match (input_ty, args) {
            (Ty::Tuple(elem_tys), Value::Tuple(arg_elems, _)) => {
                assert_eq!(
                    elem_tys.len(),
                    arg_elems.len(),
                    "tuple argument arity must match its checked type"
                );
                if elem_tys.is_empty() {
                    return make_unit_expr(package, assigner);
                }

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
            (Ty::Tuple(_), _) => panic!(
                "expected a tuple argument for checked type {input_ty}, found {}",
                args.type_name()
            ),
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

    /// Creates an empty tuple expression of type Unit.
    fn make_unit_expr(
        package: &mut qsc_fir::fir::Package,
        assigner: &mut qsc_fir::assigner::Assigner,
    ) -> qsc_fir::fir::ExprId {
        let expr_id = assigner.next_expr();
        package.exprs.insert(
            expr_id,
            qsc_fir::fir::Expr {
                id: expr_id,
                span: package.synthetic_span(),
                ty: Ty::UNIT,
                kind: qsc_fir::fir::ExprKind::Tuple(Vec::new()),
                exec_graph_range: qsc_fir::fir::ExecGraphIdx::ZERO
                    ..qsc_fir::fir::ExecGraphIdx::ZERO,
            },
        );
        expr_id
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
        callable_types: &RuntimeCallableTypes,
        pending_stmts: &mut Vec<qsc_fir::fir::StmtId>,
    ) -> qsc_fir::fir::ExprId {
        if let Some(ty @ Ty::Udt(qsc_fir::fir::Res::Item(id))) = expected_ty {
            let pure = &callable_types.udts[&(id.package, id.item).into()];
            let argument = build_synthetic_args(
                package,
                assigner,
                pure,
                value,
                callable_types,
                pending_stmts,
            );
            let constructor = assigner.next_expr();
            package.exprs.insert(
                constructor,
                qsc_fir::fir::Expr {
                    id: constructor,
                    span: package.synthetic_span(),
                    ty: Ty::Arrow(Box::new(qsc_fir::ty::Arrow {
                        kind: qsc_fir::fir::CallableKind::Function,
                        input: Box::new(pure.clone()),
                        output: Box::new(ty.clone()),
                        functors: FunctorSet::Value(qsc_fir::ty::FunctorSetValue::Empty),
                    })),
                    kind: qsc_fir::fir::ExprKind::Var(qsc_fir::fir::Res::Item(*id), Vec::new()),
                    exec_graph_range: qsc_fir::fir::ExecGraphIdx::ZERO
                        ..qsc_fir::fir::ExecGraphIdx::ZERO,
                },
            );
            let result = assigner.next_expr();
            package.exprs.insert(
                result,
                qsc_fir::fir::Expr {
                    id: result,
                    span: package.synthetic_span(),
                    ty: ty.clone(),
                    kind: qsc_fir::fir::ExprKind::Call(constructor, argument),
                    exec_graph_range: qsc_fir::fir::ExecGraphIdx::ZERO
                        ..qsc_fir::fir::ExecGraphIdx::ZERO,
                },
            );
            return result;
        }
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
            Value::Global(..) | Value::GlobalWithArgs(..) => {
                let global = value.global_instance().expect("global callable");
                return lower_global_to_expr(
                    package,
                    assigner,
                    global.id,
                    global.functor,
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
        callable_types: &RuntimeCallableTypes,
    ) -> qsc_fir::fir::ExprId {
        let info = callable_types
            .callables
            .get(&id)
            .expect("Global callable type must be pre-computed");
        let ty = info.ty.clone();
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
        callable_types: &RuntimeCallableTypes,
        pending_stmts: &mut Vec<qsc_fir::fir::StmtId>,
    ) -> qsc_fir::fir::ExprId {
        // Lifted lambdas use `(captures.., explicit_input)`, even with no captures.
        let target = closure.id;
        let full_ty = callable_types
            .callables
            .get(&target)
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
                            package: target.package,
                            item: target.item,
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
                kind: qsc_fir::fir::ExprKind::Closure(capture_locals, target.item),
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
            Value::Global(..) | Value::GlobalWithArgs(..) => {
                let global = value.global_instance().expect("global callable");
                if !callables.contains(&global.id) {
                    callables.insert(global.id);
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
    /// The fallback pins the prepared target for DCE survival so that
    /// `fir_to_qir_from_callable` can invoke it with runtime argument values.
    /// Arguments must already be compatible with the callable's signature.
    /// Runtime callables must carry their checked instantiations; missing type
    /// evidence remains a diagnostic rather than being inferred from values.
    /// This declaration-based wrapper supplies no explicit target type arguments;
    /// interpreter entry points use the typed preparation path below.
    pub fn prepare_codegen_fir_from_callable_args(
        package_store: &PackageStore,
        callable: qsc_hir::hir::ItemId,
        args: &Value,
        capabilities: TargetCapabilityFlags,
    ) -> Result<(CodegenFir, CallableArgsBackend), Vec<Error>> {
        prepare_codegen_fir_from_typed_callable_args(
            package_store,
            callable,
            &TypeArgs::Unbound,
            args,
            capabilities,
        )
    }

    /// Prepares a callable instance and its arguments using retained type bindings.
    ///
    /// Runtime specialization and the following pipeline share one budget. Only
    /// private FIR copies and rebuilt argument values are changed; source identities
    /// and interpreter-owned values remain available for later evaluation or codegen.
    pub(crate) fn prepare_codegen_fir_from_typed_callable_args(
        package_store: &PackageStore,
        callable: qsc_hir::hir::ItemId,
        type_args: &TypeArgs,
        args: &Value,
        capabilities: TargetCapabilityFlags,
    ) -> Result<(CodegenFir, CallableArgsBackend), Vec<Error>> {
        prepare_codegen_fir_from_typed_callable_args_with_policy(
            package_store,
            callable,
            type_args,
            args,
            capabilities,
            false,
        )
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn prepare_codegen_fir_from_typed_callable_args_with_policy(
        package_store: &PackageStore,
        callable: qsc_hir::hir::ItemId,
        type_args: &TypeArgs,
        args: &Value,
        capabilities: TargetCapabilityFlags,
        allow_fallback: bool,
    ) -> Result<(CodegenFir, CallableArgsBackend), Vec<Error>> {
        let mut concrete_callables = FxHashSet::default();
        collect_concrete_qsharp_callables(args, &mut concrete_callables);

        let target_callable = qsc_fir::fir::StoreItemId {
            package: qsc_lowerer::map_hir_package_to_fir(callable.package),
            item: qsc_lowerer::map_hir_local_item_to_fir(callable.item),
        };

        let source = package_store.get(callable.package).expect("source package");
        let qsc_hir::hir::ItemKind::Callable(source_decl) = &source
            .package
            .items
            .get(callable.item)
            .expect("source callable")
            .kind
        else {
            panic!("target must be callable");
        };
        if concrete_callables.is_empty() && source_decl.generics.is_empty() {
            let codegen_fir = prepare_codegen_fir_from_callable_with_policy(
                package_store,
                callable,
                capabilities,
                allow_fallback,
            )?;
            return Ok((
                codegen_fir,
                CallableArgsBackend::ReinvokeOriginal {
                    callable: target_callable,
                    args: args.clone(),
                },
            ));
        }

        concrete_callables.insert(target_callable);
        let (mut fir_store, fir_package_id, _assigner) = lower_to_fir(
            package_store,
            callable.package,
            None,
            Some(&concrete_callables),
        );

        let mut budget = qsc_fir_transforms::SpecializationBudget::default();
        let mut instantiator = RuntimeInstantiator {
            sources: package_store,
            store: &mut fir_store,
            budget: &mut budget,
            parameters: FxHashMap::default(),
            specializations: Vec::new(),
        };
        let target_callable = instantiator.instantiate_target(target_callable, type_args)?;
        let args = instantiator.instantiate_value(args)?;
        let mut concrete_callables = FxHashSet::default();
        collect_concrete_qsharp_callables(&args, &mut concrete_callables);

        // Keep runtime identities and foreign closures as values. Only their
        // private codegen implementation IDs change; payload identity is preserved.
        if !value_is_fir_lowerable(&args, target_callable.package) {
            let codegen_fir = prepare_codegen_fir_from_callable_args_pinned(
                package_store,
                fir_store,
                target_callable,
                capabilities,
                concrete_callables,
                &mut budget,
                allow_fallback,
            )?;
            return Ok((
                codegen_fir,
                CallableArgsBackend::ReinvokeOriginal {
                    callable: target_callable,
                    args,
                },
            ));
        }

        let callable_types = build_callable_type_map(&fir_store, &concrete_callables);

        // Build synthetic Call(Var(target), args) as the entry expression.
        // This makes the target and all callable args entry-reachable for pipeline transforms.
        seed_entry_with_call_to_target(
            &mut fir_store,
            fir_package_id,
            target_callable,
            &args,
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
        let warnings = run_codegen_pipeline_with_budget(
            package_store,
            &mut fir_store,
            fir_package_id,
            qsc_fir_transforms::PipelineStage::Full,
            pinned_items,
            &mut budget,
        )?;

        // Validate capabilities across the whole reachable program (the synthetic
        // entry and everything it specializes), mirroring the entry-expression path.
        let analysis = prepare_analysis(
            package_store,
            &fir_store,
            fir_package_id,
            None,
            capabilities,
            allow_fallback,
        )?;
        let fallback_reason = analysis
            .fallback_reason
            .map(|reason| map_pass_errors(package_store, callable.package, vec![reason]).remove(0));

        Ok((
            CodegenFir {
                fir_store,
                fir_package_id,
                compute_properties: analysis.compute_properties,
                capabilities: analysis.capabilities,
                fallback_reason,
                warnings,
            },
            backend,
        ))
    }

    /// Pin-based fallback for callable args containing non-lowerable closure captures.
    ///
    /// Seeds concrete (non-arrow-input) callables into the entry for reachability,
    /// pins arrow-input callables and the prepared target for DCE survival, and
    /// invokes the returned values directly during partial evaluation. A Mono-only
    /// pass first specializes their dependencies; subsequent pinned-body rewrites
    /// preserve the runtime argument layout.
    fn prepare_codegen_fir_from_callable_args_pinned(
        package_store: &PackageStore,
        mut fir_store: qsc_fir::fir::PackageStore,
        target_callable: qsc_fir::fir::StoreItemId,
        capabilities: TargetCapabilityFlags,
        mut concrete_callables: FxHashSet<qsc_fir::fir::StoreItemId>,
        budget: &mut qsc_fir_transforms::SpecializationBudget,
        allow_fallback: bool,
    ) -> Result<CodegenFir, Vec<Error>> {
        let fir_package_id = target_callable.package;
        // Specialize dependencies of pinned higher-order bodies without applying
        // signature-changing passes to those roots.
        let mut roots = concrete_callables.clone();
        roots.insert(target_callable);
        seed_entry_with_callables(&mut fir_store, fir_package_id, &roots);
        let mut warnings = run_codegen_pipeline_with_budget(
            package_store,
            &mut fir_store,
            fir_package_id,
            qsc_fir_transforms::PipelineStage::Mono,
            &[],
            budget,
        )?;
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

        fir_store.get_mut(fir_package_id).entry = None;
        seed_entry_with_callables(&mut fir_store, fir_package_id, &concrete_callables);
        if concrete_callables.is_empty() {
            let package = fir_store.get_mut(fir_package_id);
            let mut assigner = qsc_fir::assigner::Assigner::from_package(package);
            package.entry = Some(make_unit_expr(package, &mut assigner));
        }
        pinned_callables.push(target_callable);
        warnings.extend(run_codegen_pipeline_with_budget(
            package_store,
            &mut fir_store,
            fir_package_id,
            qsc_fir_transforms::PipelineStage::Full,
            &pinned_callables,
            budget,
        )?);
        // The pinned target body is not entry-reachable, so the main
        // pipeline above did not return-unify it. Re-root the body-only
        // signature-preserving sub-pipeline at the pinned callables so early
        // returns inside dynamic branches become flag-guarded forward control
        // flow. This must run BEFORE `analyze_all` so RCA sees the
        // post-return-unify shape (no `ReturnWithinDynamicScope`) and
        // the shared callable-root analysis sees the supported shape.
        warnings.extend(run_codegen_signature_preserving_subpipeline(
            package_store,
            qsc_lowerer::map_fir_package_to_hir(target_callable.package),
            &mut fir_store,
            fir_package_id,
            &pinned_callables,
        )?);
        let analysis = prepare_analysis(
            package_store,
            &fir_store,
            fir_package_id,
            Some(target_callable),
            capabilities,
            allow_fallback,
        )?;
        let fallback_reason = analysis.fallback_reason.map(|reason| {
            map_pass_errors(
                package_store,
                qsc_lowerer::map_fir_package_to_hir(fir_package_id),
                vec![reason],
            )
            .remove(0)
        });

        Ok(CodegenFir {
            fir_store,
            fir_package_id,
            compute_properties: analysis.compute_properties,
            capabilities: analysis.capabilities,
            fallback_reason,
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
            | Value::Global(..)
            | Value::GlobalWithArgs(..) => true,
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
        allow_fallback: bool,
    ) -> Result<CodegenFir, Vec<Error>> {
        let (fir_store, fir_package_id, _) =
            lower_to_fir(package_store, package_id, package_override, None);

        prepare_codegen_fir_from_lowered_store(
            package_store,
            package_id,
            fir_store,
            fir_package_id,
            capabilities,
            allow_fallback,
        )
    }

    fn prepare_codegen_fir_from_lowered_store(
        package_store: &PackageStore,
        package_id: qsc_hir::hir::PackageId,
        mut fir_store: qsc_fir::fir::PackageStore,
        fir_package_id: qsc_fir::fir::PackageId,
        capabilities: TargetCapabilityFlags,
        allow_fallback: bool,
    ) -> Result<CodegenFir, Vec<Error>> {
        let warnings = run_codegen_pipeline(package_store, &mut fir_store, fir_package_id)?;

        let analysis = prepare_analysis(
            package_store,
            &fir_store,
            fir_package_id,
            None,
            capabilities,
            allow_fallback,
        )?;
        let fallback_reason = analysis
            .fallback_reason
            .map(|reason| map_pass_errors(package_store, package_id, vec![reason]).remove(0));

        Ok(CodegenFir {
            fir_store,
            fir_package_id,
            compute_properties: analysis.compute_properties,
            capabilities: analysis.capabilities,
            fallback_reason,
            warnings,
        })
    }

    pub fn prepare_codegen_fir(
        package_store: &PackageStore,
        package_id: qsc_hir::hir::PackageId,
        capabilities: TargetCapabilityFlags,
    ) -> Result<CodegenFir, Vec<Error>> {
        prepare_codegen_fir_inner(package_store, package_id, None, capabilities, false)
    }

    pub(crate) fn prepare_qir_fir(
        package_store: &PackageStore,
        package_id: qsc_hir::hir::PackageId,
        capabilities: TargetCapabilityFlags,
    ) -> Result<CodegenFir, Vec<Error>> {
        prepare_codegen_fir_inner(package_store, package_id, None, capabilities, true)
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
            false,
        )
    }

    pub(crate) fn prepare_qir_fir_from_fir_store(
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
            true,
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
        prepare_codegen_fir_from_callable_with_policy(package_store, callable, capabilities, false)
    }

    fn prepare_codegen_fir_from_callable_with_policy(
        package_store: &PackageStore,
        callable: qsc_hir::hir::ItemId,
        capabilities: TargetCapabilityFlags,
        allow_fallback: bool,
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
                capabilities,
                fallback_reason: None,
            });
        }

        seed_entry_with_callable(&mut fir_store, fir_package_id, callable);
        let warnings = run_codegen_pipeline(package_store, &mut fir_store, fir_package_id)?;

        let analysis = prepare_analysis(
            package_store,
            &fir_store,
            fir_package_id,
            Some(qsc_fir::fir::StoreItemId {
                package: qsc_lowerer::map_hir_package_to_fir(callable.package),
                item: qsc_lowerer::map_hir_local_item_to_fir(callable.item),
            }),
            capabilities,
            allow_fallback,
        )?;
        let fallback_reason = analysis
            .fallback_reason
            .map(|reason| map_pass_errors(package_store, callable.package, vec![reason]).remove(0));

        Ok(CodegenFir {
            fir_store,
            fir_package_id,
            compute_properties: analysis.compute_properties,
            capabilities: analysis.capabilities,
            fallback_reason,
            warnings,
        })
    }

    fn compile_to_codegen_fir(
        sources: SourceMap,
        language_features: LanguageFeatures,
        capabilities: TargetCapabilityFlags,
        package_store: &mut PackageStore,
        dependencies: &Dependencies,
        allow_fallback: bool,
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
        let prepared_fir = prepare_codegen_fir_inner(
            package_store,
            package_id,
            None,
            capabilities,
            allow_fallback,
        )?;
        Ok((package_id, prepared_fir))
    }

    /// Generates QIR from an AST, retaining nonfatal diagnostics for the caller
    /// to present. Does not log or publish warnings.
    pub fn get_qir_from_ast(
        store: &mut PackageStore,
        dependencies: &Dependencies,
        ast_package: qsc_ast::ast::Package,
        sources: SourceMap,
        capabilities: TargetCapabilityFlags,
    ) -> Result<QirGenerationResult, Vec<Error>> {
        get_qir_from_ast_with_diagnostics(store, dependencies, ast_package, sources, capabilities)
    }

    /// Generates QIR and retains nonfatal diagnostics, including profile fallback.
    pub fn get_qir_from_ast_with_diagnostics(
        store: &mut PackageStore,
        dependencies: &Dependencies,
        ast_package: qsc_ast::ast::Package,
        sources: SourceMap,
        capabilities: TargetCapabilityFlags,
    ) -> Result<QirGenerationResult, Vec<Error>> {
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
        let prepared_fir = prepare_qir_fir(store, package_id, capabilities)?;
        let entry = entry_from_codegen_fir(&prepared_fir);
        let qir = fir_to_qir(
            &prepared_fir.fir_store,
            prepared_fir.capabilities,
            LanguageFeatures::default(),
            &prepared_fir.compute_properties,
            &entry,
        )
        .map_err(|e| {
            let source_package_id = e.span().package;
            let source_package = store
                .get(source_package_id)
                .expect("package should be in store");
            vec![Error::PartialEvaluation(WithSource::from_map(
                &source_package.sources,
                e,
            ))]
        })?;
        Ok(prepared_fir.generated(qir))
    }

    pub fn get_rir(
        sources: SourceMap,
        language_features: LanguageFeatures,
        capabilities: TargetCapabilityFlags,
        package_store: PackageStore,
        dependencies: &Dependencies,
    ) -> Result<Vec<String>, Vec<Error>> {
        get_rir_with_policy(
            sources,
            language_features,
            capabilities,
            package_store,
            dependencies,
            false,
        )
        .map(|result| result.rir)
    }

    /// Generates backend-preview RIR using the same bounded fallback as QIR.
    /// The original `get_rir` API remains strict for diagnostic consumers.
    pub fn get_rir_with_diagnostics(
        sources: SourceMap,
        language_features: LanguageFeatures,
        capabilities: TargetCapabilityFlags,
        package_store: PackageStore,
        dependencies: &Dependencies,
    ) -> Result<RirGenerationResult, Vec<Error>> {
        get_rir_with_policy(
            sources,
            language_features,
            capabilities,
            package_store,
            dependencies,
            true,
        )
    }

    fn get_rir_with_policy(
        sources: SourceMap,
        language_features: LanguageFeatures,
        capabilities: TargetCapabilityFlags,
        mut package_store: PackageStore,
        dependencies: &Dependencies,
        allow_fallback: bool,
    ) -> Result<RirGenerationResult, Vec<Error>> {
        let (_, prepared_fir) = compile_to_codegen_fir(
            sources,
            language_features,
            capabilities,
            &mut package_store,
            dependencies,
            allow_fallback,
        )?;
        let entry = entry_from_codegen_fir(&prepared_fir);
        let (raw, ssa) = fir_to_rir(
            &prepared_fir.fir_store,
            prepared_fir.capabilities,
            &prepared_fir.compute_properties,
            &entry,
            PartialEvalConfig {
                generate_debug_metadata: true,
                ignore_stack_limit: language_features.contains(LanguageFeatures::IgnoreStackLimit),
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
        Ok(prepared_fir.generated_rir(vec![raw.to_string(), ssa.to_string()]))
    }

    /// Generates QIR and retains nonfatal diagnostics for the caller to present.
    /// Does not log or publish warnings. A proven full-Adaptive
    /// fallback emits `Adaptive_RIF` / QIR 1.0 without recompiling source.
    pub fn get_qir(
        sources: SourceMap,
        language_features: LanguageFeatures,
        capabilities: TargetCapabilityFlags,
        package_store: PackageStore,
        dependencies: &Dependencies,
    ) -> Result<QirGenerationResult, Vec<Error>> {
        get_qir_with_diagnostics(
            sources,
            language_features,
            capabilities,
            package_store,
            dependencies,
        )
    }

    /// Generates QIR without dropping warnings. Successful fallback changes the
    /// effective backend to `Adaptive_RIF` and QIR 1.0, not frontend selection.
    pub fn get_qir_with_diagnostics(
        sources: SourceMap,
        language_features: LanguageFeatures,
        capabilities: TargetCapabilityFlags,
        mut package_store: PackageStore,
        dependencies: &Dependencies,
    ) -> Result<QirGenerationResult, Vec<Error>> {
        let (_, prepared_fir) = compile_to_codegen_fir(
            sources,
            language_features,
            capabilities,
            &mut package_store,
            dependencies,
            true,
        )?;
        let entry = entry_from_codegen_fir(&prepared_fir);
        let qir = fir_to_qir(
            &prepared_fir.fir_store,
            prepared_fir.capabilities,
            language_features,
            &prepared_fir.compute_properties,
            &entry,
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
        Ok(prepared_fir.generated(qir))
    }
}
