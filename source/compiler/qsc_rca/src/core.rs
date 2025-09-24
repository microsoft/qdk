// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    ApplicationGeneratorSet, ArrayParamApplication, ComputeKind, ComputePropertiesLookup,
    ParamApplication, QuantumProperties, RuntimeFeatureFlags, RuntimeKind, ValueKind,
    applications::{ApplicationInstance, GeneratorSetsBuilder, LocalComputeKind},
    common::{
        AssignmentStmtCounter, Callee, FunctorAppExt, GlobalSpecId, Local, LocalKind, TyExt,
        try_resolve_callee,
    },
    scaffolding::{InternalItemComputeProperties, InternalPackageStoreComputeProperties},
};
use qsc_data_structures::{functors::FunctorApp, index_map::IndexMap};
use qsc_fir::{
    extensions::InputParam,
    fir::{
        Attr, BinOp, Block, BlockId, CallableDecl, CallableImpl, CallableKind, Expr, ExprId,
        ExprKind, FieldAssign, Global, Ident, Item, ItemKind, LocalVarId, Mutability, Package,
        PackageId, PackageLookup, PackageStore, PackageStoreLookup, Pat, PatId, PatKind, Res,
        SpecDecl, SpecImpl, Stmt, StmtId, StmtKind, StoreExprId, StoreItemId, StorePatId,
        StringComponent,
    },
    ty::{Arrow, FunctorSetValue, Prim, Ty},
    visit::{Visitor, walk_stmt},
};

pub struct Analyzer<'a> {
    package_store: &'a PackageStore,
    package_store_compute_properties: InternalPackageStoreComputeProperties,
    active_contexts: Vec<AnalysisContext>,
}

impl<'a> Analyzer<'a> {
    pub fn new(
        package_store: &'a PackageStore,
        package_store_compute_properties: InternalPackageStoreComputeProperties,
    ) -> Self {
        Self {
            package_store,
            package_store_compute_properties,
            active_contexts: Vec::<AnalysisContext>::default(),
        }
    }

    pub fn analyze_all(mut self) -> InternalPackageStoreComputeProperties {
        for (package_id, package) in self.package_store {
            self.analyze_package_internal(package_id, package);
        }
        self.package_store_compute_properties
    }

    pub fn analyze_package(
        mut self,
        package_id: PackageId,
    ) -> InternalPackageStoreComputeProperties {
        let package = self.package_store.get(package_id);
        self.analyze_package_internal(package_id, package);
        self.package_store_compute_properties
    }

    fn analyze_expr_array(&mut self, exprs: &Vec<ExprId>) -> ComputeKind {
        // Visit each sub-expression in the array to determine their compute kind, and aggregate ONLY the runtime
        // features to the array's compute kind.
        let default_value_kind = ValueKind::Array(RuntimeKind::Static, RuntimeKind::Static);
        let mut compute_kind = ComputeKind::Classical;
        let mut has_dynamic_content = false;
        for expr_id in exprs {
            self.visit_expr(*expr_id);
            let application_instance = self.get_current_application_instance();
            let expr_compute_kind = application_instance.get_expr_compute_kind(*expr_id);
            compute_kind =
                compute_kind.aggregate_runtime_features(*expr_compute_kind, default_value_kind);
            has_dynamic_content |= expr_compute_kind.is_dynamic();
        }

        // The value kind of an array expression has two components. The runtime value of its content and the runtime
        // value of its size. For array expressions, the runtime value of its content depend on whether any of its
        // elements is dynamic, and the runtime value of its size is always static.
        if has_dynamic_content {
            let ComputeKind::Quantum(quantum_properties) = &mut compute_kind else {
                panic!(
                    "the compute kind of an array expression cannot have dynamic content and be classical"
                );
            };

            quantum_properties.value_kind =
                ValueKind::Array(RuntimeKind::Dynamic, RuntimeKind::Static);
        }

        compute_kind
    }

    fn analyze_expr_array_repeat(
        &mut self,
        value_expr_id: ExprId,
        size_expr_id: ExprId,
    ) -> ComputeKind {
        // Visit the value and size expressions to determine their compute kind.
        self.visit_expr(value_expr_id);
        self.visit_expr(size_expr_id);

        // The runtime features the array repeat expression is determined by aggregating the runtime features of both
        // the size and value expressions.
        let application_instance = self.get_current_application_instance();
        let size_expr_compute_kind = *application_instance.get_expr_compute_kind(size_expr_id);
        let value_expr_compute_kind = *application_instance.get_expr_compute_kind(value_expr_id);
        let default_value_kind = ValueKind::Array(RuntimeKind::Static, RuntimeKind::Static);
        let mut compute_kind = ComputeKind::Classical;
        compute_kind =
            compute_kind.aggregate_runtime_features(size_expr_compute_kind, default_value_kind);
        compute_kind =
            compute_kind.aggregate_runtime_features(value_expr_compute_kind, default_value_kind);

        if let ComputeKind::Quantum(quantum_properties) = &mut compute_kind {
            // If the array is dynamic, it requires an additional runtime feature.
            if size_expr_compute_kind.is_dynamic() {
                quantum_properties.runtime_features |=
                    RuntimeFeatureFlags::UseOfDynamicallySizedArray;
            }

            // The value kind of an array expression has two components. The runtime kind of its content and the runtime
            // kind of its size. For array repeat expressions, the runtime kind of its content depend on whether the
            // value expression is dynamic, and the runtime kind of its size depend on whether the size expression is
            // dynamic.
            let content_runtime_value = if value_expr_compute_kind.is_dynamic() {
                RuntimeKind::Dynamic
            } else {
                RuntimeKind::Static
            };
            let size_runtime_value = if size_expr_compute_kind.is_dynamic() {
                RuntimeKind::Dynamic
            } else {
                RuntimeKind::Static
            };
            quantum_properties.value_kind =
                ValueKind::Array(content_runtime_value, size_runtime_value);
        }

        compute_kind
    }

    fn analyze_expr_assign(
        &mut self,
        assignee_expr_id: ExprId,
        value_expr_id: ExprId,
    ) -> ComputeKind {
        // Visit the assignee and value expressions to determine their compute kind.
        self.visit_expr(assignee_expr_id);
        self.visit_expr(value_expr_id);

        // Since this is an assignment, update the local variables on the assignee expression with the compute kind of
        // the value expression.
        let updated_compute_kind = self.update_locals_compute_kind(assignee_expr_id, value_expr_id);

        // We do not care about the value kind for this kind of expression because it is an assignment, but we still
        // need a default one.
        let default_value_kind = ValueKind::Element(RuntimeKind::Static);
        let mut compute_kind = ComputeKind::Classical;

        // The compute kind of an assign expression is determined by the runtime features of the updated compute kind
        // associated to the local variable.
        compute_kind =
            compute_kind.aggregate_runtime_features(updated_compute_kind, default_value_kind);
        compute_kind
    }

    fn analyze_expr_assign_index(
        &mut self,
        array_var_expr_id: ExprId,
        index_expr_id: ExprId,
        replacement_value_expr_id: ExprId,
    ) -> ComputeKind {
        // Visit the array variable, index and replacement value expressions to determine their compute kind.
        self.visit_expr(array_var_expr_id);
        self.visit_expr(index_expr_id);
        self.visit_expr(replacement_value_expr_id);

        // Since this is an assignment, the compute kind of the local variable (array var expression) needs to be updated.
        // The compute kind of the update is determined by the runtime features of the replacement value expression.
        let application_instance = self.get_current_application_instance();
        let mut replacement_value_compute_kind =
            *application_instance.get_expr_compute_kind(replacement_value_expr_id);

        let mut default_value_kind = ValueKind::Array(RuntimeKind::Static, RuntimeKind::Static);
        // If we are within a dynamic scope, the compute kind of the assign index expression is dynamic and an additional
        // runtime feature is used to mark the array itself as dynamically sized.
        if !application_instance.active_dynamic_scopes.is_empty() {
            default_value_kind = ValueKind::Array(RuntimeKind::Dynamic, RuntimeKind::Dynamic);
            let replacement_ty = &self.get_expr(replacement_value_expr_id).ty;
            replacement_value_compute_kind =
                replacement_value_compute_kind.aggregate(ComputeKind::new_with_runtime_features(
                    RuntimeFeatureFlags::UseOfDynamicallySizedArray,
                    ValueKind::new_static_from_type(replacement_ty),
                ));
        }

        let mut updated_compute_kind = ComputeKind::Classical;
        updated_compute_kind = updated_compute_kind
            .aggregate_runtime_features(replacement_value_compute_kind, default_value_kind);

        // If the replacement value expression is dynamic, the runtime features and value kind of the update have to
        // take this into account.
        if replacement_value_compute_kind.is_dynamic() {
            let ComputeKind::Quantum(quantum_properties) = &mut updated_compute_kind else {
                panic!(
                    "the compute kind of the update must be quantum if the replacement value is dynamic"
                );
            };

            let ValueKind::Array(content_runtime_value, _) = &mut quantum_properties.value_kind
            else {
                panic!("the value kind of the update must be an array variant");
            };

            *content_runtime_value = RuntimeKind::Dynamic;
        }

        // Update the compute kind of the local variable in the locals map.
        let array_var_expr = self.get_expr(array_var_expr_id);
        let ExprKind::Var(Res::Local(local_var_id), _) = &array_var_expr.kind else {
            panic!("LHS expression should be a local");
        };
        let application_instance = self.get_current_application_instance_mut();
        application_instance
            .locals_map
            .aggregate_compute_kind(*local_var_id, updated_compute_kind);

        // The compute kind of this expression is determined by aggregating the runtime features of the index and
        // replacement expressions.
        // We do not care about the value kind for this kind of expression because it is an assignment, but we still
        // need a default one.
        let default_value_kind = ValueKind::Element(RuntimeKind::Static);
        let index_compute_kind = *application_instance.get_expr_compute_kind(index_expr_id);
        let mut compute_kind = ComputeKind::Classical;
        compute_kind =
            compute_kind.aggregate_runtime_features(index_compute_kind, default_value_kind);
        compute_kind = compute_kind
            .aggregate_runtime_features(replacement_value_compute_kind, default_value_kind);

        // Finally, if the index expression is dynamic, we aggregate an additional runtime feature.
        if index_compute_kind.is_dynamic() {
            compute_kind = compute_kind.aggregate_runtime_features(
                ComputeKind::new_with_runtime_features(
                    RuntimeFeatureFlags::UseOfDynamicIndex,
                    default_value_kind,
                ),
                default_value_kind,
            );
        }
        compute_kind
    }

    fn analyze_expr_bin_op(
        &mut self,
        lhs_expr_id: ExprId,
        rhs_expr_id: ExprId,
        expr_type: &Ty,
    ) -> ComputeKind {
        // Visit the LHS and RHS expressions to determine their compute kind.
        self.visit_expr(lhs_expr_id);
        self.visit_expr(rhs_expr_id);

        // The compute kind of a binary operator expression is the aggregation of its LHS and RHS expressions.
        let application_instance = self.get_current_application_instance();
        let lhs_compute_kind = *application_instance.get_expr_compute_kind(lhs_expr_id);
        let rhs_compute_kind = *application_instance.get_expr_compute_kind(rhs_expr_id);
        let mut compute_kind = ComputeKind::Classical;
        compute_kind = compute_kind.aggregate(lhs_compute_kind);
        compute_kind = compute_kind.aggregate(rhs_compute_kind);

        // Additionally, since the new compute kind can be of a different type than its operands (e.g. 1 == 1),
        // aggregate additional runtime features depending on the binary operator expression's type (if it's dynamic).
        if let Some(value_kind) = compute_kind.value_kind() {
            let ComputeKind::Quantum(quantum_properties) = &mut compute_kind else {
                panic!("expected quantum variant of compute kind");
            };

            quantum_properties.runtime_features |=
                derive_runtime_features_for_value_kind_associated_to_type(value_kind, expr_type);
        }

        compute_kind
    }

    fn analyze_expr_bin_op_exp(&mut self, lhs_expr_id: ExprId, rhs_expr_id: ExprId) -> ComputeKind {
        // Visit the LHS and RHS expressions to determine their compute kind.
        self.visit_expr(lhs_expr_id);
        self.visit_expr(rhs_expr_id);

        // The compute kind of a binary operator expression is the aggregation of its LHS and RHS expressions.
        let application_instance = self.get_current_application_instance();
        let lhs_compute_kind = *application_instance.get_expr_compute_kind(lhs_expr_id);
        let rhs_compute_kind = *application_instance.get_expr_compute_kind(rhs_expr_id);
        let mut compute_kind = ComputeKind::Classical;
        compute_kind = compute_kind.aggregate(lhs_compute_kind);
        compute_kind = compute_kind.aggregate(rhs_compute_kind);

        // As a special case for exp, if the rhs is dynamic the expression gets an extra runtime feature.
        // This is because we don't emit a native exponential binary operator but unroll it into a sequence
        // of multiplications, which requires the rhs to be known at compile time.
        if rhs_compute_kind.is_dynamic() {
            compute_kind = compute_kind.aggregate_runtime_features(
                ComputeKind::new_with_runtime_features(
                    RuntimeFeatureFlags::UseOfDynamicExponent,
                    ValueKind::Element(RuntimeKind::Static),
                ),
                ValueKind::Element(RuntimeKind::Static),
            );
        }

        compute_kind
    }

    fn analyze_expr_block(&mut self, block_id: BlockId) -> ComputeKind {
        // Visit the block to determine its compute kind.
        self.visit_block(block_id);

        // The compute kind of a block expression is the same as the compute kind of the block.
        let application_instance = self.get_current_application_instance();
        *application_instance.get_block_compute_kind(block_id)
    }

    fn analyze_expr_call(
        &mut self,
        callee_expr_id: ExprId,
        args_expr_id: ExprId,
        expr_type: &Ty,
    ) -> ComputeKind {
        // Visit the callee and arguments expressions to determine their compute kind.
        self.visit_expr(callee_expr_id);
        self.visit_expr(args_expr_id);

        // The compute kind of this expression depends on whether the callee expression is dynamic.
        let application_instance = self.get_current_application_instance();
        let callee_expr_compute_kind = *application_instance.get_expr_compute_kind(callee_expr_id);
        let mut compute_kind = if callee_expr_compute_kind.is_dynamic() {
            // The value kind of a call expression with an dynamic callee is dynamic but its specific variant depends
            // on the expression's type.
            let value_kind = ValueKind::new_dynamic_from_type(expr_type);
            ComputeKind::Quantum(QuantumProperties {
                runtime_features: RuntimeFeatureFlags::CallToDynamicCallee,
                value_kind,
            })
        } else {
            let call_compute_kind =
                self.analyze_expr_call_with_static_callee(callee_expr_id, args_expr_id, expr_type);
            match call_compute_kind {
                CallComputeKind::Regular(compute_kind) => compute_kind,
                CallComputeKind::Override(compute_kind) => {
                    return compute_kind;
                }
            }
        };

        // If this call happens within a dynamic scope, there might be additional runtime features being used.
        let default_value_kind = ValueKind::new_static_from_type(expr_type);
        let application_instance = self.get_current_application_instance();
        if !application_instance.active_dynamic_scopes.is_empty() {
            // If the call expression type is either a result or a qubit, it uses dynamic allocation runtime features.
            if let Ty::Prim(Prim::Qubit) = expr_type {
                // We consider this qubit dynamic so the value kind of this expression must be dynamic.
                compute_kind = compute_kind.aggregate(ComputeKind::Quantum(QuantumProperties {
                    runtime_features: RuntimeFeatureFlags::empty(),
                    value_kind: ValueKind::Element(RuntimeKind::Dynamic),
                }));
            }

            if let Ty::Prim(Prim::Result) = expr_type {
                compute_kind = compute_kind.aggregate_runtime_features(
                    ComputeKind::new_with_runtime_features(
                        RuntimeFeatureFlags::MeasurementWithinDynamicScope,
                        default_value_kind,
                    ),
                    default_value_kind,
                );
            }
        }

        // If the call expression is dynamic, aggregate the corresponding runtime features depending on its type.
        if let Some(value_kind) = compute_kind.value_kind() {
            let ComputeKind::Quantum(quantum_properties) = &mut compute_kind else {
                panic!("expected quantum variant of Compute Kind");
            };
            quantum_properties.runtime_features |=
                derive_runtime_features_for_value_kind_associated_to_type(value_kind, expr_type);
        }

        // Aggregate the runtime features of the callee and arguments expressions.
        let callee_expr_compute_kind = *application_instance.get_expr_compute_kind(callee_expr_id);
        let args_expr_compute_kind = *application_instance.get_expr_compute_kind(args_expr_id);
        compute_kind =
            compute_kind.aggregate_runtime_features(callee_expr_compute_kind, default_value_kind);
        compute_kind =
            compute_kind.aggregate_runtime_features(args_expr_compute_kind, default_value_kind);
        compute_kind
    }

    fn analyze_expr_call_for_length_intrinsic(&self, args_expr_id: ExprId) -> ComputeKind {
        let application_instance = self.get_current_application_instance();
        let args_compute_kind = *application_instance.get_expr_compute_kind(args_expr_id);
        match args_compute_kind {
            ComputeKind::Classical => ComputeKind::Classical,
            ComputeKind::Quantum(quantum_properties) => {
                if quantum_properties
                    .runtime_features
                    .contains(RuntimeFeatureFlags::UseOfDynamicallySizedArray)
                {
                    ComputeKind::new_with_runtime_features(
                        quantum_properties.runtime_features,
                        ValueKind::Element(RuntimeKind::Dynamic),
                    )
                } else {
                    ComputeKind::Classical
                }
            }
        }
    }

    fn analyze_expr_call_with_spec_callee(
        &mut self,
        callee: &Callee,
        callable_decl: &'a CallableDecl,
        args_expr_id: ExprId,
        expr_type: &Ty,
        fixed_args: Option<Vec<LocalVarId>>,
    ) -> CallComputeKind {
        // The `Length` intrinsic function has a specialized override.
        if is_length_intrinsic(callable_decl) {
            return CallComputeKind::Override(
                self.analyze_expr_call_for_length_intrinsic(args_expr_id),
            );
        }

        // Analyze the specialization to determine its application generator set.
        let callee_id = GlobalSpecId::from((callee.item, callee.functor_app.functor_set_value()));
        self.analyze_spec(callee_id, callable_decl);
        let application_generator_set = self.package_store_compute_properties.get_spec(callee_id);

        // We need to split controls and specialization input arguments so we can derive the correct callable
        // application.
        let package_id = self.get_current_package_id();
        let args_package = self.package_store.get(package_id);
        let (args_controls, args_input_id) =
            split_controls_and_input(args_expr_id, callee.functor_app, args_package);

        // To map the input pattern to input expressions we need to provide global (store-level) pattern and expression
        // identifiers since the callable can be in a different package than the input expressions.
        let callee_input_pattern_id =
            StorePatId::from((callee_id.callable.package, callable_decl.input));
        let args_input_id = StoreExprId::from((package_id, args_input_id));
        let arg_exprs = map_input_pattern_to_input_expressions(
            callee_input_pattern_id,
            args_input_id,
            self.package_store,
            fixed_args.as_ref().map(Vec::len),
        );
        let application_instance = self.get_current_application_instance();

        // Derive the compute kind based on the value kind of the arguments.
        let arg_value_kinds = if let Some(fixed_args) = fixed_args {
            // In items that come from lifted lambdas, fixed arguments that capture local variables, if any, come before
            // other arguments, so we use the `fixed_args` as the base of the chain of values and concatenate the rest of
            // the arguments when building the full list of arguments for a callable application.
            fixed_args
                .into_iter()
                .map(|local_var_id| {
                    self.get_current_application_instance()
                        .locals_map
                        .find_local_compute_kind(local_var_id)
                        .map_or(ComputeKind::Classical, |v| v.compute_kind)
                        .value_kind_or_default(ValueKind::Element(RuntimeKind::Static))
                })
                .chain(self.derive_arg_value_kinds(&arg_exprs))
                .collect()
        } else {
            self.derive_arg_value_kinds(&arg_exprs)
        };
        let mut compute_kind =
            application_generator_set.generate_application_compute_kind(&arg_value_kinds);

        // Aggregate the runtime features of the qubit controls expressions.
        let mut has_dynamic_controls = false;
        let default_value_kind = ValueKind::new_static_from_type(&callable_decl.output);
        for control_expr in args_controls {
            let control_expr_compute_kind =
                *application_instance.get_expr_compute_kind(control_expr);
            compute_kind = compute_kind
                .aggregate_runtime_features(control_expr_compute_kind, default_value_kind);
            has_dynamic_controls |= control_expr_compute_kind.is_dynamic();
        }

        // If any of the control expressions is dynamic, set the compute kind of the call expression to the
        // corresponding dynamic variant.
        if has_dynamic_controls {
            let value_kind = ValueKind::new_dynamic_from_type(&callable_decl.output);
            compute_kind.aggregate_value_kind(value_kind);
        }

        // To distinguish between a cyclic operation and a call to a cyclic operation, replace the cyclic operation
        // runtime feature (if any) by a call to a cyclic operation.
        if let ComputeKind::Quantum(quantum_properties) = &mut compute_kind {
            if quantum_properties
                .runtime_features
                .contains(RuntimeFeatureFlags::CyclicOperationSpec)
            {
                quantum_properties
                    .runtime_features
                    .remove(RuntimeFeatureFlags::CyclicOperationSpec);
                quantum_properties
                    .runtime_features
                    .insert(RuntimeFeatureFlags::CallToCyclicOperation);
            }
        }

        // If the callable output has type parameters, there might be a discrepancy in the value kind variant we derive
        // from the application generator set and the value kind variant that corresponds to the call expression type.
        // Fix that discrepancy here.
        if callable_decl.output.has_type_parameters() {
            if let ComputeKind::Quantum(quantum_properties) = &mut compute_kind {
                // Create a default value kind for the call expression type just to know which variant we should map to.
                // Then map the currently computed variant onto it.
                let mut mapped_value_kind = ValueKind::new_static_from_type(expr_type);
                quantum_properties
                    .value_kind
                    .project_onto_variant(&mut mapped_value_kind);
                quantum_properties.value_kind = mapped_value_kind;
            }
        }
        CallComputeKind::Regular(compute_kind)
    }

    fn analyze_expr_call_with_static_callee(
        &mut self,
        callee_expr_id: ExprId,
        args_expr_id: ExprId,
        expr_type: &Ty,
    ) -> CallComputeKind {
        // Try to resolve the callee.
        let package_id = self.get_current_package_id();
        let package = self.package_store.get(package_id);
        let application_instance = self.get_current_application_instance();
        let maybe_callee = try_resolve_callee(
            callee_expr_id,
            package_id,
            package,
            &application_instance.locals_map,
        );

        // If the callee could not be resolved, return a compute kind with certain runtime features.
        let (Some(callee), fixed_args) = maybe_callee else {
            // The value kind of a call expression with an unresolved callee is not known, so to avoid
            // spurious errors in later analysis where the value is used we assume static.
            // During partial-evaluation, the callable is known the actual return kind will be checked.
            let value_kind = ValueKind::new_static_from_type(expr_type);
            let compute_kind = ComputeKind::Quantum(QuantumProperties {
                runtime_features: RuntimeFeatureFlags::CallToUnresolvedCallee,
                value_kind,
            });
            self.get_current_application_instance_mut()
                .unresolved_callee_exprs
                .push(callee_expr_id);
            return CallComputeKind::Regular(compute_kind);
        };

        if self.callee_in_active_contexts(&callee) {
            assert_eq!(
                expr_type,
                &Ty::UNIT,
                "output type for allowed recursive call should be Unit"
            );

            // This is a recursive call, which we allow with some deferred capabilities checks at runtime.
            // We treat the call as an unresolved callee, like above, such that partial evaluation will perform
            // extra validation on the capabilities at runtime.
            // This covers the corner case where a recursive call is made with a dynamic argument whose
            // type is allowed to be dynamic but whose usage in later recursion could require additional
            // capabilities.
            self.get_current_application_instance_mut()
                .unresolved_callee_exprs
                .push(callee_expr_id);
            return CallComputeKind::Regular(ComputeKind::Quantum(QuantumProperties {
                runtime_features: RuntimeFeatureFlags::CallToUnresolvedCallee,
                value_kind: ValueKind::Element(RuntimeKind::Static),
            }));
        }

        // We could resolve the callee. Determine the compute kind of the call depending on the callee kind.
        let Some(global_callee) = self.package_store.get_global(callee.item) else {
            // If the callee is not found, that is an indication that it is an item that was removed during
            // incremental compilation but remains in the name resolution data structures. Assume it is classical
            // so that it generates an "unbound name" error at runtime.
            return CallComputeKind::Regular(ComputeKind::Classical);
        };
        match global_callee {
            Global::Callable(callable_decl) => self.analyze_expr_call_with_spec_callee(
                &callee,
                callable_decl,
                args_expr_id,
                expr_type,
                fixed_args,
            ),
            Global::Udt => {
                CallComputeKind::Regular(self.analyze_expr_call_with_udt_callee(args_expr_id))
            }
        }
    }

    fn analyze_expr_call_with_udt_callee(&self, args_expr_id: ExprId) -> ComputeKind {
        let application_instance = self.get_current_application_instance();
        let args_expr_compute_kind = *application_instance.get_expr_compute_kind(args_expr_id);

        // To determine the compute kind of an UDT call expression, aggregate the runtime features of the arguments
        // expression.
        let default_value_kind = ValueKind::Element(RuntimeKind::Static);
        let mut compute_kind = ComputeKind::Classical;
        compute_kind =
            compute_kind.aggregate_runtime_features(args_expr_compute_kind, default_value_kind);

        // If any argument to the UDT constructor is dynamic, then the UDT instance is also dynamic and uses an
        // additional runtime feature.
        if args_expr_compute_kind.is_dynamic() {
            compute_kind.aggregate_value_kind(ValueKind::Element(RuntimeKind::Dynamic));
        }

        compute_kind
    }

    fn analyze_expr_fail(&mut self, msg_expr_id: ExprId) -> ComputeKind {
        // Visit the message expression to determine its compute kind.
        self.visit_expr(msg_expr_id);

        // The compute kind of the expression is determined from the message expression runtime features plus an
        // additional runtime feature if the message expresion is dynamic.
        let application_instance = self.get_current_application_instance();
        let msg_expr_compute_kind = *application_instance.get_expr_compute_kind(msg_expr_id);
        let mut compute_kind = ComputeKind::Classical;
        let default_value_kind = ValueKind::Element(RuntimeKind::Static);
        compute_kind =
            compute_kind.aggregate_runtime_features(msg_expr_compute_kind, default_value_kind);

        compute_kind
    }

    fn analyze_expr_field(&mut self, record_expr_id: ExprId, expr_type: &Ty) -> ComputeKind {
        // Visit the record expression to determine its compute kind.
        self.visit_expr(record_expr_id);

        // The compute kind of the field expression is determined from the runtime features of the record expression and
        // the value kind adapted to the expression's type.
        let application_instance = self.get_current_application_instance();
        let record_expr_compute_kind = *application_instance.get_expr_compute_kind(record_expr_id);
        let value_kind = if record_expr_compute_kind.is_dynamic() {
            ValueKind::new_dynamic_from_type(expr_type)
        } else {
            ValueKind::new_static_from_type(expr_type)
        };

        let mut compute_kind = ComputeKind::Classical;
        compute_kind =
            compute_kind.aggregate_runtime_features(record_expr_compute_kind, value_kind);
        compute_kind
    }

    fn analyze_expr_if(
        &mut self,
        condition_expr_id: ExprId,
        body_expr_id: ExprId,
        otherwise_expr_id: Option<ExprId>,
        expr_type: &Ty,
    ) -> ComputeKind {
        // Visit the condition expression to determine its compute kind.
        self.visit_expr(condition_expr_id);

        // If the condition expression is dynamic, we push a new dynamic scope.
        let application_instance = self.get_current_application_instance_mut();
        let condition_expr_compute_kind =
            *application_instance.get_expr_compute_kind(condition_expr_id);
        let within_dynamic_scope = condition_expr_compute_kind.is_dynamic();
        if within_dynamic_scope {
            application_instance
                .active_dynamic_scopes
                .push(condition_expr_id);
        }

        // Visit the body and otherwise expressions to determine their compute kind.
        self.visit_expr(body_expr_id);
        otherwise_expr_id.iter().for_each(|e| self.visit_expr(*e));

        // Pop the dynamic scope.
        if within_dynamic_scope {
            let application_instance = self.get_current_application_instance_mut();
            let dynamic_scope_expr_id = application_instance
                .active_dynamic_scopes
                .pop()
                .expect("at least one dynamic scope should exist");
            assert!(dynamic_scope_expr_id == condition_expr_id);
        }

        // Aggregate the runtime features of the sub-expressions.
        let application_instance = self.get_current_application_instance();
        let default_value_kind = ValueKind::new_static_from_type(expr_type);
        let mut compute_kind = ComputeKind::Classical;
        let condition_expr_compute_kind =
            *application_instance.get_expr_compute_kind(condition_expr_id);
        compute_kind = compute_kind
            .aggregate_runtime_features(condition_expr_compute_kind, default_value_kind);
        let body_expr_compute_kind = *application_instance.get_expr_compute_kind(body_expr_id);
        compute_kind =
            compute_kind.aggregate_runtime_features(body_expr_compute_kind, default_value_kind);
        if let Some(otherwise_expr_id) = otherwise_expr_id {
            let otherwise_expr_compute_kind =
                *application_instance.get_expr_compute_kind(otherwise_expr_id);
            compute_kind = compute_kind
                .aggregate_runtime_features(otherwise_expr_compute_kind, default_value_kind);
        }

        // If any of the sub-expressions is dynamic, then the compute kind of an if-expression is dynamic and additional
        // runtime features are aggregated.
        let is_any_sub_expr_dynamic = condition_expr_compute_kind.is_dynamic()
            || body_expr_compute_kind.is_dynamic()
            || otherwise_expr_id
                .is_some_and(|e| application_instance.get_expr_compute_kind(e).is_dynamic());
        if is_any_sub_expr_dynamic {
            let dynamic_value_kind = if matches!(expr_type, Ty::Array(..)) {
                // An array coming from a dynamic conditional should be treated as dynamic in length
                // and content.
                ValueKind::Array(
                    RuntimeKind::Dynamic,
                    if condition_expr_compute_kind.is_dynamic() {
                        RuntimeKind::Dynamic
                    } else {
                        RuntimeKind::Static
                    },
                )
            } else {
                ValueKind::new_dynamic_from_type(expr_type)
            };
            let mut dynamic_runtime_features =
                derive_runtime_features_for_value_kind_associated_to_type(
                    dynamic_value_kind,
                    expr_type,
                );
            if condition_expr_compute_kind.is_dynamic() {
                if is_any_result(expr_type) {
                    dynamic_runtime_features |= RuntimeFeatureFlags::UseOfDynamicResult;
                }
                if matches!(expr_type, Ty::Tuple(tup) if !tup.is_empty()) {
                    dynamic_runtime_features |= RuntimeFeatureFlags::UseOfDynamicTuple;
                }
            }
            let dynamic_compute_kind = ComputeKind::Quantum(QuantumProperties {
                runtime_features: dynamic_runtime_features,
                value_kind: dynamic_value_kind,
            });
            compute_kind = compute_kind.aggregate(dynamic_compute_kind);
        }

        compute_kind
    }

    fn analyze_expr_index(
        &mut self,
        array_expr_id: ExprId,
        index_expr_id: ExprId,
        expr_type: &Ty,
    ) -> ComputeKind {
        // Visit the array and index expressions to determine their compute kind.
        self.visit_expr(array_expr_id);
        self.visit_expr(index_expr_id);

        // The runtime features of the access by index expression are determined by aggregating the runtime features of
        // the array expression and the index expression.
        let application_instance = self.get_current_application_instance();
        let array_expr_compute_kind = *application_instance.get_expr_compute_kind(array_expr_id);
        let index_expr_compute_kind = *application_instance.get_expr_compute_kind(index_expr_id);
        let default_value_kind = ValueKind::new_static_from_type(expr_type);
        let mut compute_kind = ComputeKind::Classical;
        compute_kind =
            compute_kind.aggregate_runtime_features(array_expr_compute_kind, default_value_kind);
        compute_kind =
            compute_kind.aggregate_runtime_features(index_expr_compute_kind, default_value_kind);

        // If the index expression is dynamic, the value kind of the expression is also dynamic and an additional
        // runtime feature is used.
        if let ComputeKind::Quantum(index_quantum_properties) = &index_expr_compute_kind {
            let ValueKind::Element(index_runtime_value) = index_quantum_properties.value_kind
            else {
                panic!("the value kind of an index expression must be of the element variant");
            };

            if matches!(index_runtime_value, RuntimeKind::Dynamic) {
                let dynamic_runtime_features = RuntimeFeatureFlags::UseOfDynamicIndex;
                let dynamic_value_kind = ValueKind::new_dynamic_from_type(expr_type);
                compute_kind = compute_kind.aggregate(ComputeKind::Quantum(QuantumProperties {
                    runtime_features: dynamic_runtime_features,
                    value_kind: dynamic_value_kind,
                }));
            }
        }

        // The value kind of the access by index expression also depends on whether the content of the array expression
        // is dynamic.
        if let ComputeKind::Quantum(array_quantum_properties) = &array_expr_compute_kind {
            let ValueKind::Array(content_runtime_kind, _) = array_quantum_properties.value_kind
            else {
                panic!("the value kind of an array expression must be of the array variant");
            };

            if matches!(content_runtime_kind, RuntimeKind::Dynamic) {
                let dynamic_value_kind = ValueKind::new_dynamic_from_type(expr_type);
                compute_kind.aggregate_value_kind(dynamic_value_kind);
            }
        }

        // If the index expression is dynamic, aggregate the corresponding runtime features depending on its type.
        if let Some(value_kind) = compute_kind.value_kind() {
            let ComputeKind::Quantum(quantum_properties) = &mut compute_kind else {
                panic!("expected quantum variant of Compute Kind");
            };
            quantum_properties.runtime_features |=
                derive_runtime_features_for_value_kind_associated_to_type(value_kind, expr_type);
        }

        compute_kind
    }

    fn analyze_expr_range(
        &mut self,
        start_expr_id: Option<ExprId>,
        step_expr_id: Option<ExprId>,
        end_expr_id: Option<ExprId>,
        expr_type: &Ty,
    ) -> ComputeKind {
        // Visit the start, step and end expressions to determine their compute kind.
        start_expr_id.iter().for_each(|e| self.visit_expr(*e));
        step_expr_id.iter().for_each(|e| self.visit_expr(*e));
        end_expr_id.iter().for_each(|e| self.visit_expr(*e));

        // The compute kind of a range expression is the aggregation of its start, step and end expressions.
        let application_instance = self.get_current_application_instance();
        let start_expr_compute_kind = start_expr_id.map_or(ComputeKind::Classical, |e| {
            *application_instance.get_expr_compute_kind(e)
        });
        let step_expr_compute_kind = step_expr_id.map_or(ComputeKind::Classical, |e| {
            *application_instance.get_expr_compute_kind(e)
        });
        let end_expr_compute_kind = end_expr_id.map_or(ComputeKind::Classical, |e| {
            *application_instance.get_expr_compute_kind(e)
        });
        let mut compute_kind = ComputeKind::Classical;
        compute_kind = compute_kind.aggregate(start_expr_compute_kind);
        compute_kind = compute_kind.aggregate(step_expr_compute_kind);
        compute_kind = compute_kind.aggregate(end_expr_compute_kind);

        // Additionally, if the compute kind of the range is dynamic, mark it with the appropriate runtime feature.
        if compute_kind.is_dynamic() {
            let static_value_kind = ValueKind::new_static_from_type(expr_type);
            compute_kind = compute_kind.aggregate(ComputeKind::new_with_runtime_features(
                RuntimeFeatureFlags::UseOfDynamicRange,
                static_value_kind,
            ));
        }
        compute_kind
    }

    fn analyze_expr_return(&mut self, value_expr_id: ExprId) -> ComputeKind {
        // Visit the value expression to determine its compute kind.
        self.visit_expr(value_expr_id);

        // The compute kind of the return expression depends on whether the expression happens within a dynamic scope.
        let application_instance = self.get_current_application_instance();
        let mut compute_kind = if application_instance.active_dynamic_scopes.is_empty() {
            ComputeKind::Classical
        } else {
            ComputeKind::Quantum(QuantumProperties {
                runtime_features: RuntimeFeatureFlags::ReturnWithinDynamicScope,
                value_kind: ValueKind::Element(RuntimeKind::Static),
            })
        };

        // Now just aggregate the runtime features of the value expression.
        let default_value_kind = ValueKind::Element(RuntimeKind::Static);
        let value_expr_compute_kind = *application_instance.get_expr_compute_kind(value_expr_id);
        compute_kind =
            compute_kind.aggregate_runtime_features(value_expr_compute_kind, default_value_kind);
        compute_kind
    }

    fn analyze_expr_struct(
        &mut self,
        copy: Option<ExprId>,
        fields: &[FieldAssign],
        expr_type: &Ty,
    ) -> ComputeKind {
        let default_value_kind = ValueKind::Element(RuntimeKind::Static);
        let mut compute_kind = ComputeKind::Classical;
        let mut has_dynamic_sub_exprs = false;
        if let Some(copy_expr_id) = copy {
            // Visit the copy expression to determine its compute kind.
            self.visit_expr(copy_expr_id);
            let application_instance = self.get_current_application_instance();
            let expr_compute_kind = *application_instance.get_expr_compute_kind(copy_expr_id);
            compute_kind =
                compute_kind.aggregate_runtime_features(expr_compute_kind, default_value_kind);
            has_dynamic_sub_exprs |= expr_compute_kind.is_dynamic();
        }

        // Visit the fields to determine their compute kind.
        for expr_id in fields.iter().map(|field| field.value) {
            self.visit_expr(expr_id);
            let application_instance = self.get_current_application_instance();
            let expr_compute_kind = *application_instance.get_expr_compute_kind(expr_id);
            compute_kind =
                compute_kind.aggregate_runtime_features(expr_compute_kind, default_value_kind);
            has_dynamic_sub_exprs |= expr_compute_kind.is_dynamic();
        }

        // If any of the sub-expressions are dynamic, then the struct expression is dynamic as well.
        if has_dynamic_sub_exprs {
            compute_kind.aggregate_value_kind(ValueKind::Element(RuntimeKind::Dynamic));
        }

        // If the constructor is dynamic, aggregate the corresponding runtime features depending on its type.
        if let ComputeKind::Quantum(quantum_properties) = &mut compute_kind {
            quantum_properties.runtime_features |=
                derive_runtime_features_for_value_kind_associated_to_type(
                    quantum_properties.value_kind,
                    expr_type,
                );
        }

        compute_kind
    }

    fn analyze_expr_string(&mut self, components: &Vec<StringComponent>) -> ComputeKind {
        // Visit the string components to determine their compute kind, aggregate its runtime features and track whether
        // any of them is dynamic to construct the compute kind of the string expression itself.
        let default_value_kind = ValueKind::Element(RuntimeKind::Static);
        let mut has_dynamic_components = false;
        let mut compute_kind = ComputeKind::Classical;
        for component in components {
            match component {
                StringComponent::Expr(expr_id) => {
                    self.visit_expr(*expr_id);
                    let application_instance = self.get_current_application_instance();
                    let component_compute_kind =
                        *application_instance.get_expr_compute_kind(*expr_id);
                    compute_kind = compute_kind
                        .aggregate_runtime_features(component_compute_kind, default_value_kind);
                    has_dynamic_components |= component_compute_kind.is_dynamic();
                }
                StringComponent::Lit(_) => {
                    // Nothing to aggregate.
                }
            }
        }

        // If any of the string components is dynamic, then the string expression is dynamic as well.
        if has_dynamic_components {
            let ComputeKind::Quantum(quantum_properties) = &mut compute_kind else {
                panic!("Quantum variant was expected for the compute kind of string expression ");
            };
            quantum_properties.runtime_features |= RuntimeFeatureFlags::UseOfDynamicString;
            quantum_properties.value_kind = ValueKind::Element(RuntimeKind::Dynamic);
        }

        compute_kind
    }

    fn analyze_expr_tuple(&mut self, exprs: &Vec<ExprId>) -> ComputeKind {
        // Visit the sub-expressions to determine their compute kind, aggregate its runtime features and track whether
        // any of them is dynamic to construct the compute kind of the tuple expression itself.
        let default_value_kind = ValueKind::Element(RuntimeKind::Static);
        let mut compute_kind = ComputeKind::Classical;
        let mut has_dynamic_sub_exprs = false;
        for expr_id in exprs {
            self.visit_expr(*expr_id);
            let application_instance = self.get_current_application_instance();
            let expr_compute_kind = *application_instance.get_expr_compute_kind(*expr_id);
            compute_kind =
                compute_kind.aggregate_runtime_features(expr_compute_kind, default_value_kind);
            has_dynamic_sub_exprs |= expr_compute_kind.is_dynamic();
        }

        // If any of the sub-expressions is dynamic, then the tuple expression is dynamic as well.
        if has_dynamic_sub_exprs {
            compute_kind.aggregate_value_kind(ValueKind::Element(RuntimeKind::Dynamic));
        }

        compute_kind
    }

    fn analyze_expr_un_op(&mut self, operand_expr_id: ExprId) -> ComputeKind {
        // Visit the operand expression to determine its compute kind.
        self.visit_expr(operand_expr_id);

        // The compute kind of an unary expression is the same as the compute kind of its operand expression.
        let application_instance = self.get_current_application_instance();
        *application_instance.get_expr_compute_kind(operand_expr_id)
    }

    fn analyze_expr_update_field(
        &mut self,
        record_expr_id: ExprId,
        replace_expr_id: ExprId,
    ) -> ComputeKind {
        // Visit the record and replace expressions to determine their compute kind.
        self.visit_expr(record_expr_id);
        self.visit_expr(replace_expr_id);

        // The runtime features of an update field expression are determined by aggregating the runtime features of the
        // record and replace expressions.
        let application_instance = self.get_current_application_instance();
        let record_expr_compute_kind = *application_instance.get_expr_compute_kind(record_expr_id);
        let replace_expr_compute_kind =
            *application_instance.get_expr_compute_kind(replace_expr_id);
        let default_value_kind = ValueKind::Element(RuntimeKind::Static);
        let mut compute_kind = ComputeKind::Classical;
        compute_kind =
            compute_kind.aggregate_runtime_features(record_expr_compute_kind, default_value_kind);
        compute_kind =
            compute_kind.aggregate_runtime_features(replace_expr_compute_kind, default_value_kind);

        // If either the record or the replace expressions are dynamic, the update field expression is dynamic as well.
        if record_expr_compute_kind.is_dynamic() || replace_expr_compute_kind.is_dynamic() {
            compute_kind.aggregate_value_kind(ValueKind::Element(RuntimeKind::Dynamic));
        }

        compute_kind
    }

    fn analyze_expr_update_index(
        &mut self,
        array_expr_id: ExprId,
        index_expr_id: ExprId,
        replacement_value_expr_id: ExprId,
    ) -> ComputeKind {
        // Visit the array, index and replacement value expressions to determine their compute kind.
        self.visit_expr(array_expr_id);
        self.visit_expr(index_expr_id);
        self.visit_expr(replacement_value_expr_id);

        // The runtime features of an update index expression is determined by aggregating the runtime features of its
        // sub-expressions, with some nuanced considerations.
        let application_instance = self.get_current_application_instance();
        let array_expr_compute_kind = *application_instance.get_expr_compute_kind(array_expr_id);
        let index_expr_compute_kind = *application_instance.get_expr_compute_kind(index_expr_id);
        let replacement_value_expr_compute_kind =
            *application_instance.get_expr_compute_kind(replacement_value_expr_id);
        let default_value_kind = ValueKind::Array(RuntimeKind::Static, RuntimeKind::Static);
        let mut compute_kind = ComputeKind::Classical;
        compute_kind =
            compute_kind.aggregate_runtime_features(array_expr_compute_kind, default_value_kind);
        compute_kind =
            compute_kind.aggregate_runtime_features(index_expr_compute_kind, default_value_kind);
        compute_kind = compute_kind
            .aggregate_runtime_features(replacement_value_expr_compute_kind, default_value_kind);

        // If the index expression is dynamic, an additional runtime feature is used.
        if index_expr_compute_kind.is_dynamic() {
            let additional_compute_kind = ComputeKind::Quantum(QuantumProperties {
                runtime_features: RuntimeFeatureFlags::UseOfDynamicIndex,
                value_kind: default_value_kind,
            });
            compute_kind = compute_kind
                .aggregate_runtime_features(additional_compute_kind, default_value_kind);
        }

        // The value kind of the update index expression is based on the value kind of the array expression.
        if let ComputeKind::Quantum(array_quantum_properties) = array_expr_compute_kind {
            compute_kind.aggregate_value_kind(array_quantum_properties.value_kind);
        }

        // If either the index or the replacement value expressions are dynamic, then the content of the resulting array
        // expression is also dynamic.
        if index_expr_compute_kind.is_dynamic() || replacement_value_expr_compute_kind.is_dynamic()
        {
            let content_value_kind = ValueKind::Array(RuntimeKind::Dynamic, RuntimeKind::Static);
            compute_kind.aggregate_value_kind(content_value_kind);
        }

        compute_kind
    }

    fn analyze_expr_var(&self, res: &Res) -> ComputeKind {
        match res {
            // Global items do not have quantum properties by themselves so we can consider them classical.
            Res::Item(_) => ComputeKind::Classical,
            // Gather the current compute kind of the local.
            Res::Local(local_var_id) => {
                let application_instance = self.get_current_application_instance();
                application_instance
                    .locals_map
                    .find_local_compute_kind(*local_var_id)
                    .map_or(ComputeKind::Classical, |v| v.compute_kind)
            }
            Res::Err => panic!("unexpected error resolution"),
        }
    }

    fn analyze_expr_while(&mut self, condition_expr_id: ExprId, block_id: BlockId) -> ComputeKind {
        // Visit the condition expression to determine its initial compute kind.
        self.visit_expr(condition_expr_id);
        let application_instance = self.get_current_application_instance_mut();
        let mut condition_expr_compute_kind =
            *application_instance.get_expr_compute_kind(condition_expr_id);

        // We analyze both the condition expression and the block N times, where N is the analysis stabilization limit.
        // The reason why we need a stabilization limit is because there can be up-to N levels of indirection for the
        // condition due to variable assigments.
        // The number of statements with assignments in the condition expression and the loop block is a good proxy for
        // the worst case scenario regarding the propagation of properties throughout variables. Because of this, we use
        // it as the stabilization limit.
        let package_id = self.get_current_package_id();
        let package = self.package_store.get(package_id);
        let stabilization_limit = AssignmentStmtCounter::new(package).count_in_block(block_id)
            + AssignmentStmtCounter::new(package).count_in_expr(condition_expr_id);
        for _ in 0..=stabilization_limit {
            // If the condition expression is dynamic, we push a new dynamic scope before visiting the block.
            let application_instance = self.get_current_application_instance_mut();
            condition_expr_compute_kind =
                *application_instance.get_expr_compute_kind(condition_expr_id);
            let within_dynamic_scope = condition_expr_compute_kind.is_dynamic();
            if within_dynamic_scope {
                application_instance
                    .active_dynamic_scopes
                    .push(condition_expr_id);
            }
            self.visit_expr(condition_expr_id);
            self.visit_block(block_id);
            if within_dynamic_scope {
                let application_instance = self.get_current_application_instance_mut();
                let dynamic_scope_expr_id = application_instance
                    .active_dynamic_scopes
                    .pop()
                    .expect("at least one dynamic scope should exist");
                assert!(dynamic_scope_expr_id == condition_expr_id);
            }
        }

        // Return the aggregated runtime features of the condition expression and the block.
        let application_instance = self.get_current_application_instance();
        let block_compute_kind = *application_instance.get_block_compute_kind(block_id);
        let default_value_kind = ValueKind::Element(RuntimeKind::Static);
        let mut compute_kind = ComputeKind::Classical;
        compute_kind = compute_kind
            .aggregate_runtime_features(condition_expr_compute_kind, default_value_kind);
        compute_kind =
            compute_kind.aggregate_runtime_features(block_compute_kind, default_value_kind);

        // If the condition is dynamic, we require an additional runtime feature.
        if condition_expr_compute_kind.is_dynamic() {
            let ComputeKind::Quantum(quantum_properties) = &mut compute_kind else {
                panic!("if the loop condition is quantum, the loop expression must be quantum too");
            };
            quantum_properties.runtime_features |= RuntimeFeatureFlags::LoopWithDynamicCondition;
        }

        compute_kind
    }

    // Analyzes the currently active callable assuming it is intrinsic.
    fn analyze_intrinsic_callable(&mut self) {
        // Check whether the callable has already been analyzed.
        let current_item_context = self.get_current_item_context();
        let body_specialization_id =
            GlobalSpecId::from((current_item_context.id, FunctorSetValue::Empty));
        if self
            .package_store_compute_properties
            .find_specialization(body_specialization_id)
            .is_some()
        {
            return;
        }

        // Determine the application generator set depending on whether the callable is a function or an operation.
        let callable_context = current_item_context.get_callable_context();
        let application_generator_set = match callable_context.kind {
            CallableKind::Function => {
                derive_intrinsic_function_application_generator_set(callable_context)
            }
            CallableKind::Operation => {
                derive_instrinsic_operation_application_generator_set(callable_context)
            }
        };

        // Insert the generator set in the entry corresponding to the body specialization of the callable.
        self.package_store_compute_properties
            .insert_spec(body_specialization_id, application_generator_set);
    }

    fn analyze_item(&mut self, item_id: StoreItemId, item: &'a Item) {
        self.push_item_context(item_id);
        self.visit_item(item);
        let popped_item_id = self.pop_item_context();
        assert!(popped_item_id == item_id);
    }

    fn analyze_package_internal(&mut self, package_id: PackageId, package: &'a Package) {
        // Analyze all top level items.
        for (local_item_id, item) in &package.items {
            self.analyze_item((package_id, local_item_id).into(), item);
        }

        // Analyze top-level statements, which should be the only ones unanalyzed at this point.
        let unanalyzed_stmts = self.unanalyzed_stmts(package_id);
        self.push_top_level_context(package_id);
        for stmt_id in unanalyzed_stmts {
            // Visit the statement to determine its compute kind.
            self.visit_stmt(stmt_id);
        }

        // Analyze the entry expression.
        if let Some(entry_expr_id) = package.entry {
            self.visit_expr(entry_expr_id);

            // An entry expression includes runtime flags for each primitive type in its return type
            // that must be used in output recording.
            let entry_ty = self.get_expr(entry_expr_id).ty.clone();
            let ty_flags = ty_to_runtime_runtime_output_flags(&entry_ty);
            if !ty_flags.is_empty() {
                let mut entry_compute_kind = *self
                    .get_current_application_instance()
                    .get_expr_compute_kind(entry_expr_id);
                if let ComputeKind::Quantum(quantum_properties) = &mut entry_compute_kind {
                    quantum_properties.runtime_features |= ty_flags;
                } else {
                    entry_compute_kind = ComputeKind::Quantum(QuantumProperties {
                        runtime_features: ty_flags,
                        value_kind: ValueKind::new_static_from_type(&entry_ty),
                    });
                }
                self.get_current_application_instance_mut()
                    .insert_expr_compute_kind(entry_expr_id, entry_compute_kind);
            }
        }
        let top_level_context = self.pop_top_level_context();
        assert!(top_level_context.package_id == package_id);

        // Save the analysis of the top-level elements to the corresponding package compute properties.
        let package_compute_properties = self.package_store_compute_properties.get_mut(package_id);
        top_level_context
            .builder
            .save_to_package_compute_properties(package_compute_properties, None);
    }

    fn analyze_spec(&mut self, id: GlobalSpecId, callable_decl: &'a CallableDecl) {
        // Only do this if the specialization has not been analyzed already.
        if self
            .package_store_compute_properties
            .find_specialization(id)
            .is_some()
        {
            return;
        }

        // Push the context of the callable the specialization belongs to.
        self.push_item_context(id.callable);
        let package = self.package_store.get(id.callable.package);
        let input_params = package.derive_callable_input_params(callable_decl);
        let current_callable_context = self.get_current_item_context_mut();
        current_callable_context.set_callable_context(
            callable_decl.kind,
            input_params,
            callable_decl.output.clone(),
            &callable_decl.attrs,
        );

        // Continue with the analysis differently depending on whether the callable is an intrinsic or not.
        match &callable_decl.implementation {
            CallableImpl::Intrinsic | CallableImpl::SimulatableIntrinsic(_) => {
                self.analyze_intrinsic_callable();
            }
            CallableImpl::Spec(spec_impl) => {
                // Only analyze the specialization that corresponds to the provided ID. Otherwise, we can get into an
                // infinite analysis loop.
                let spec_decl = match id.functor_set_value {
                    FunctorSetValue::Empty => &spec_impl.body,
                    FunctorSetValue::Adj => spec_impl
                        .adj
                        .as_ref()
                        .expect("adj specialization should exist"),
                    FunctorSetValue::Ctl => spec_impl
                        .ctl
                        .as_ref()
                        .expect("ctl specialization should exist"),
                    FunctorSetValue::CtlAdj => spec_impl
                        .ctl_adj
                        .as_ref()
                        .expect("ctladj specialization should exist"),
                };
                self.analyze_spec_decl(spec_decl, id.functor_set_value);
            }
        }

        // Since we are done analyzing the specialization, pop the active item context.
        let popped_item_id = self.pop_item_context();
        assert!(popped_item_id == id.callable);
    }

    fn analyze_spec_decl(&mut self, decl: &'a SpecDecl, functor_set_value: FunctorSetValue) {
        // Only do this if the specialization has not been analyzed already.
        let current_item_context = self.get_current_item_context();
        let global_spec_id = GlobalSpecId::from((current_item_context.id, functor_set_value));
        if self
            .package_store_compute_properties
            .find_specialization(global_spec_id)
            .is_some()
        {
            return;
        }

        // Set the context for the specialization declaration, visit it and then clear the context to get the results
        // of the analysis.
        let package_id = self.get_current_package_id();
        self.set_current_spec_context(decl, functor_set_value);
        self.visit_spec_decl(decl);
        let spec_context = self.clear_current_spec_context();
        assert!(spec_context.functor_set_value == functor_set_value);

        // Save the analysis to the corresponding package compute properties.
        let package_compute_properties = self.package_store_compute_properties.get_mut(package_id);
        let application_generator_set = spec_context
            .builder
            .save_to_package_compute_properties(package_compute_properties, Some(decl.block))
            .expect("applications generator set should be some");
        self.package_store_compute_properties
            .insert_spec(global_spec_id, application_generator_set);
    }

    fn bind_compute_kind_to_ident(
        &mut self,
        ident: &Ident,
        local_kind: LocalKind,
        compute_kind: ComputeKind,
    ) {
        let application_instance = self.get_current_application_instance_mut();
        let local = Local {
            var: ident.id,
            kind: local_kind,
        };
        let local_compute_kind = LocalComputeKind {
            local,
            compute_kind,
        };
        application_instance
            .locals_map
            .insert(ident.id, local_compute_kind);
    }

    fn bind_expr_compute_kind_to_pattern(
        &mut self,
        mutability: Mutability,
        pat_id: PatId,
        expr_id: ExprId,
    ) {
        let expr = self.get_expr(expr_id);
        let pat = self.get_pat(pat_id);
        match &pat.kind {
            PatKind::Bind(ident) => {
                let local_kind = match mutability {
                    Mutability::Immutable => LocalKind::Immutable(expr_id),
                    Mutability::Mutable => LocalKind::Mutable,
                };
                let application_instance = self.get_current_application_instance();
                let expr_compute_kind = *application_instance.get_expr_compute_kind(expr_id);
                let bound_compute_kind = ComputeKind::map_to_type(expr_compute_kind, &pat.ty);
                self.bind_compute_kind_to_ident(ident, local_kind, bound_compute_kind);
            }
            PatKind::Tuple(pats) => match &expr.kind {
                ExprKind::Tuple(exprs) => {
                    for (pat_id, expr_id) in pats.iter().zip(exprs.iter()) {
                        self.bind_expr_compute_kind_to_pattern(mutability, *pat_id, *expr_id);
                    }
                }
                _ => {
                    self.bind_fixed_expr_compute_kind_to_pattern(mutability, pat_id, expr_id);
                }
            },
            PatKind::Discard => {
                // Nothing to bind to.
            }
        }
    }

    fn bind_fixed_expr_compute_kind_to_pattern(
        &mut self,
        mutability: Mutability,
        pat_id: PatId,
        expr_id: ExprId,
    ) {
        let pat = self.get_pat(pat_id);
        match &pat.kind {
            PatKind::Bind(ident) => {
                let local_kind = match mutability {
                    Mutability::Immutable => LocalKind::Immutable(expr_id),
                    Mutability::Mutable => LocalKind::Mutable,
                };
                let application_instance = self.get_current_application_instance();
                let expr_compute_kind = *application_instance.get_expr_compute_kind(expr_id);
                let bound_compute_kind = ComputeKind::map_to_type(expr_compute_kind, &pat.ty);
                self.bind_compute_kind_to_ident(ident, local_kind, bound_compute_kind);
            }
            PatKind::Tuple(pats) => {
                for pat_id in pats {
                    self.bind_fixed_expr_compute_kind_to_pattern(mutability, *pat_id, expr_id);
                }
            }
            PatKind::Discard => {
                // Nothing to bind to.
            }
        }
    }

    fn clear_current_spec_context(&mut self) -> SpecContext {
        self.get_current_item_context_mut()
            .clear_current_spec_context()
    }

    fn derive_arg_value_kinds(&self, args: &Vec<ExprId>) -> Vec<ValueKind> {
        let application_instance = self.get_current_application_instance();
        let mut args_value_kinds = Vec::<ValueKind>::with_capacity(args.len());
        for arg_expr_id in args {
            let arg_compute_kind = application_instance.get_expr_compute_kind(*arg_expr_id);
            let arg_expr = self.get_expr(*arg_expr_id);
            let default_value_kind = ValueKind::new_static_from_type(&arg_expr.ty);
            let arg_value_kind = arg_compute_kind.value_kind_or_default(default_value_kind);
            args_value_kinds.push(arg_value_kind);
        }
        args_value_kinds
    }

    fn get_current_application_instance(&self) -> &ApplicationInstance {
        self.get_current_context()
            .get_current_application_instance()
    }

    fn get_current_application_instance_mut(&mut self) -> &mut ApplicationInstance {
        self.get_current_context_mut()
            .get_current_application_instance_mut()
    }

    fn get_current_context(&self) -> &AnalysisContext {
        self.active_contexts
            .last()
            .expect("there are no active contexts")
    }

    fn get_current_context_mut(&mut self) -> &mut AnalysisContext {
        self.active_contexts
            .last_mut()
            .expect("there are no active contexts")
    }

    fn get_current_item_context(&self) -> &ItemContext {
        let current_context = self.get_current_context();
        let AnalysisContext::Item(item_context) = &current_context else {
            panic!("the current analysis context is not an item context");
        };
        item_context
    }

    fn get_current_item_context_mut(&mut self) -> &mut ItemContext {
        let current_context = self.get_current_context_mut();
        let AnalysisContext::Item(item_context) = current_context else {
            panic!("the current analysis context is not an item context");
        };
        item_context
    }

    fn get_current_spec_context_mut(&mut self) -> &mut SpecContext {
        self.get_current_item_context_mut()
            .get_current_spec_context_mut()
    }

    fn get_current_package_id(&self) -> PackageId {
        let current_context = self.get_current_context();
        match current_context {
            AnalysisContext::TopLevel(top_level_context) => top_level_context.package_id,
            AnalysisContext::Item(item_context) => item_context.id.package,
        }
    }

    fn pop_item_context(&mut self) -> StoreItemId {
        let popped_context = self
            .active_contexts
            .pop()
            .expect("there are no active contexts");
        let AnalysisContext::Item(item_context) = popped_context else {
            panic!("the current analysis context is not an item context");
        };
        item_context.id
    }

    fn pop_top_level_context(&mut self) -> TopLevelContext {
        let popped_context = self
            .active_contexts
            .pop()
            .expect("there are no active contexts");
        let AnalysisContext::TopLevel(top_level_context) = popped_context else {
            panic!("the current analysis context is not an top-level context");
        };
        top_level_context
    }

    fn push_item_context(&mut self, id: StoreItemId) {
        self.active_contexts
            .push(AnalysisContext::Item(ItemContext::new(id)));
    }

    fn push_top_level_context(&mut self, package_id: PackageId) {
        self.active_contexts
            .push(AnalysisContext::TopLevel(TopLevelContext::new(package_id)));
    }

    fn set_current_spec_context(&mut self, decl: &'a SpecDecl, functor_set_value: FunctorSetValue) {
        assert!(
            self.get_current_item_context()
                .current_spec_context
                .is_none()
        );
        let package_id = self.get_current_package_id();
        let pats = &self.package_store.get(package_id).pats;
        let input_params = self.get_current_item_context().get_input_params();
        let controls = derive_specialization_controls(decl, pats);
        let callable_conext = self.get_current_item_context().get_callable_context();
        let spec_context = SpecContext::new(
            functor_set_value,
            input_params,
            controls.as_ref(),
            &callable_conext.output_type,
        );
        self.get_current_item_context_mut()
            .set_current_spec_context(spec_context);
    }

    fn unanalyzed_stmts(&self, package_id: PackageId) -> Vec<StmtId> {
        let package = self.package_store.get(package_id);
        let mut unanalyzed_stmts = Vec::new();
        for (stmt_id, _) in &package.stmts {
            if self
                .package_store_compute_properties
                .find_stmt((package_id, stmt_id).into())
                .is_none()
            {
                unanalyzed_stmts.push(stmt_id);
            }
        }
        unanalyzed_stmts
    }

    fn update_locals_compute_kind(
        &mut self,
        assignee_expr_id: ExprId,
        value_expr_id: ExprId,
    ) -> ComputeKind {
        let assignee_expr = self.get_expr(assignee_expr_id);
        let value_expr = self.get_expr(value_expr_id);
        match &assignee_expr.kind {
            ExprKind::Var(res, _) => {
                let Res::Local(local_var_id) = res else {
                    panic!("expected a local variable");
                };

                // The updated compute kind is the aggregation of the compute kind of the local variable and the
                // assigned value.
                // Start by initializing the updated compute kind with the compute kind of the local variable.
                let application_instance = self.get_current_application_instance_mut();
                let local_var_compute_kind = application_instance
                    .locals_map
                    .get_or_init_local_compute_kind(
                        *local_var_id,
                        LocalKind::Mutable,
                        ComputeKind::Classical,
                    );
                let mut updated_compute_kind = local_var_compute_kind.compute_kind;

                // Since the local variable compute kind is what will be updated, the value kind must match the local
                // variable's type. That is why before aggregating the compute kind of the assigned value we need to get
                // a default value kind of the matching type.
                // In some cases, there might be some loss of granularity on the value kind (e.g. assigning an array to
                // a UDT variable field since we do not track individual UDT fields).
                let value_expr_compute_kind =
                    *application_instance.get_expr_compute_kind(value_expr_id);
                let assigned_compute_kind =
                    ComputeKind::map_to_type(value_expr_compute_kind, &assignee_expr.ty);
                updated_compute_kind = updated_compute_kind.aggregate(assigned_compute_kind);

                // If a local is updated within a dynamic scope, the updated value of the local variable should be
                // dynamic and additional runtime features may apply.
                if !application_instance.active_dynamic_scopes.is_empty() {
                    let local_type = &assignee_expr.ty;
                    let mut dynamic_value_kind = ValueKind::new_dynamic_from_type(local_type);
                    let mut dynamic_runtime_features =
                        derive_runtime_features_for_value_kind_associated_to_type(
                            dynamic_value_kind,
                            local_type,
                        );
                    update_features_for_type(
                        local_type,
                        &mut dynamic_runtime_features,
                        &mut dynamic_value_kind,
                    );
                    let dynamic_compute_kind = ComputeKind::new_with_runtime_features(
                        dynamic_runtime_features,
                        dynamic_value_kind,
                    );
                    updated_compute_kind = updated_compute_kind.aggregate(dynamic_compute_kind);
                }

                // If the updated compute kind is dynamic, include additional properties depending on the type of the
                // local variable.
                if let Some(value_kind) = updated_compute_kind.value_kind() {
                    let ComputeKind::Quantum(updated_quantum_properties) =
                        &mut updated_compute_kind
                    else {
                        panic!("expected Quantum variant of Compute Kind");
                    };
                    updated_quantum_properties.runtime_features |=
                        derive_runtime_features_for_value_kind_associated_to_type(
                            value_kind,
                            &assignee_expr.ty,
                        );
                }

                let application_instance = self.get_current_application_instance_mut();
                application_instance
                    .locals_map
                    .aggregate_compute_kind(*local_var_id, updated_compute_kind);
                updated_compute_kind
            }
            ExprKind::Tuple(assignee_exprs) => {
                if let ExprKind::Tuple(value_exprs) = &value_expr.kind {
                    assert!(assignee_exprs.len() == value_exprs.len());

                    // To determine the update compute kind, we aggregate the runtime features of each element.
                    let default_value_kind = ValueKind::new_static_from_type(&value_expr.ty);
                    let mut updated_compute_kind = ComputeKind::Classical;
                    for (element_assignee_expr_id, element_value_expr_id) in
                        assignee_exprs.iter().zip(value_exprs.iter())
                    {
                        let element_update_compute_kind = self.update_locals_compute_kind(
                            *element_assignee_expr_id,
                            *element_value_expr_id,
                        );
                        updated_compute_kind = updated_compute_kind.aggregate_runtime_features(
                            element_update_compute_kind,
                            default_value_kind,
                        );
                    }
                    updated_compute_kind
                } else {
                    // To determine the update compute kind, we aggregate the runtime features of each update.
                    let default_value_kind = ValueKind::new_static_from_type(&value_expr.ty);
                    let mut updated_compute_kind = ComputeKind::Classical;
                    for element_assignee_expr_id in assignee_exprs {
                        let element_update_compute_kind = self
                            .update_locals_compute_kind(*element_assignee_expr_id, value_expr_id);
                        updated_compute_kind = updated_compute_kind.aggregate_runtime_features(
                            element_update_compute_kind,
                            default_value_kind,
                        );
                    }
                    updated_compute_kind
                }
            }
            _ => panic!("expected a local variable or a tuple"),
        }
    }

    /// Sets the application generator set for all statements in a block (including nested statements)
    /// to the default without analyzing them. This is done to prevent the statements from being
    /// treated as top-level statements by later analysis assumptions.
    fn set_all_stmts_in_block_to_default(&mut self, block_id: BlockId) {
        struct StmtCollector<'a> {
            package: &'a Package,
            stmts: Vec<StmtId>,
        }
        impl<'a> StmtCollector<'a> {
            fn new(package: &'a Package) -> Self {
                Self {
                    package,
                    stmts: Vec::new(),
                }
            }
        }
        impl<'a> Visitor<'a> for StmtCollector<'a> {
            fn get_block(&self, id: BlockId) -> &'a Block {
                self.package.get_block(id)
            }
            fn get_expr(&self, id: ExprId) -> &'a Expr {
                self.package.get_expr(id)
            }
            fn get_pat(&self, id: PatId) -> &'a Pat {
                self.package.get_pat(id)
            }
            fn get_stmt(&self, id: StmtId) -> &'a Stmt {
                self.package.get_stmt(id)
            }
            fn visit_stmt(&mut self, stmt_id: StmtId) {
                self.stmts.push(stmt_id);
                walk_stmt(self, stmt_id);
            }
        }

        let current_package = self.package_store.get(self.get_current_package_id());
        let mut stmt_collector = StmtCollector::new(current_package);
        stmt_collector.visit_block(block_id);
        for stmt_id in stmt_collector.stmts {
            self.package_store_compute_properties.insert_stmt(
                (self.get_current_package_id(), stmt_id).into(),
                ApplicationGeneratorSet::default(),
            );
        }
    }

    /// Checks whether the given callee is present in any of the active contexts, which indicates we are about
    /// to perform a call into a cycle.
    fn callee_in_active_contexts(&self, callee: &Callee) -> bool {
        self.active_contexts.iter().any(|context| {
            matches!(context, AnalysisContext::Item(item)
                if item.id == callee.item
                    && item.current_spec_context.as_ref().map(|s| s.functor_set_value) == Some(callee.functor_app.functor_set_value()))
        })
    }
}

fn update_features_for_type(
    local_type: &Ty,
    dynamic_runtime_features: &mut RuntimeFeatureFlags,
    dynamic_value_kind: &mut ValueKind,
) {
    match local_type {
        Ty::Array(..) => {
            // For arrays updated in a dynamic context, we also need to include the runtime feature
            // of dynamic arrays and change the value kind.
            *dynamic_runtime_features |= RuntimeFeatureFlags::UseOfDynamicallySizedArray;
            *dynamic_value_kind = ValueKind::Array(RuntimeKind::Dynamic, RuntimeKind::Dynamic);
        }
        Ty::Tuple(tup) if !tup.is_empty() => {
            // For tuples updated in a dynamic context, we also need to include the runtime feature
            // of dynamic tuples and change the value kind.
            *dynamic_runtime_features |= RuntimeFeatureFlags::UseOfDynamicTuple;
            *dynamic_value_kind = ValueKind::Element(RuntimeKind::Dynamic);
        }
        Ty::Prim(Prim::Result) => {
            // For result types updated in a dynamic context, we need to include the runtime
            // feature of dynamic results.
            *dynamic_runtime_features |= RuntimeFeatureFlags::UseOfDynamicResult;
        }
        _ => {}
    }
}

impl<'a> Visitor<'a> for Analyzer<'a> {
    fn get_block(&self, id: BlockId) -> &'a Block {
        let package_id = self.get_current_package_id();
        self.package_store.get_block((package_id, id).into())
    }

    fn get_expr(&self, id: ExprId) -> &'a Expr {
        let package_id = self.get_current_package_id();
        self.package_store.get_expr((package_id, id).into())
    }

    fn get_pat(&self, id: PatId) -> &'a Pat {
        let package_id = self.get_current_package_id();
        self.package_store.get_pat((package_id, id).into())
    }

    fn get_stmt(&self, id: StmtId) -> &'a Stmt {
        let package_id = self.get_current_package_id();
        self.package_store.get_stmt((package_id, id).into())
    }

    fn visit_block(&mut self, block_id: BlockId) {
        // Visiting a block always happens in the context of an application instance.
        let block = self.get_block(block_id);

        // Visit each statement in the block and aggregate its compute kind.
        let default_value_kind = ValueKind::new_static_from_type(&block.ty);
        let mut block_compute_kind = ComputeKind::Classical;
        for stmt_id in &block.stmts {
            // Visiting a statement performs its analysis for the current application instance.
            self.visit_stmt(*stmt_id);

            // Now, we can query the statement's compute kind and aggregate it to the block's compute kind.
            let application_instance = self.get_current_application_instance();
            let stmt_compute_kind = *application_instance.get_stmt_compute_kind(*stmt_id);
            block_compute_kind = block_compute_kind
                .aggregate_runtime_features(stmt_compute_kind, default_value_kind);
        }

        // Update the block's value kind if its non-unit, based on the value kind of its last statement's expression.
        if block.ty != Ty::UNIT {
            let last_stmt_id = block
                .stmts
                .last()
                .expect("block should have at least one statement");
            let last_stmt = self.get_stmt(*last_stmt_id);
            if let StmtKind::Expr(last_expr_id) = last_stmt.kind {
                let application_instance = self.get_current_application_instance();
                let last_expr_compute_kind =
                    application_instance.get_expr_compute_kind(last_expr_id);
                if let ComputeKind::Quantum(last_expr_quantum_properties) = last_expr_compute_kind {
                    let mut block_value_kind = ValueKind::new_static_from_type(&block.ty);
                    last_expr_quantum_properties
                        .value_kind
                        .project_onto_variant(&mut block_value_kind);
                    block_compute_kind.aggregate_value_kind(block_value_kind);
                }
            }
        }

        // Finally, insert the block's compute kind to the application instance.
        let application_instance = self.get_current_application_instance_mut();
        application_instance.insert_block_compute_kind(block_id, block_compute_kind);
    }

    fn visit_callable_decl(&mut self, decl: &'a CallableDecl) {
        let package_id = self.get_current_package_id();

        // Derive the input parameters of the callable and add them to the currently active callable.
        let package = self.package_store.get(package_id);
        let input_params = package.derive_callable_input_params(decl);
        let current_callable_context = self.get_current_item_context_mut();
        current_callable_context.set_callable_context(
            decl.kind,
            input_params,
            decl.output.clone(),
            &decl.attrs,
        );
        self.visit_callable_impl(&decl.implementation);
    }

    fn visit_callable_impl(&mut self, callable_impl: &'a CallableImpl) {
        let current_item_context = self.get_current_item_context();
        let is_test_callable = current_item_context
            .callable_context
            .as_ref()
            .is_some_and(|f| f.attrs.contains(&Attr::Test));
        match callable_impl {
            CallableImpl::Intrinsic => {
                self.analyze_intrinsic_callable();
            }
            CallableImpl::SimulatableIntrinsic(spec_decl) => {
                // Treat this as an intrinsic callable.
                self.analyze_intrinsic_callable();
                // Additionally, mark all the statements so that they appear visited.
                self.set_all_stmts_in_block_to_default(spec_decl.block);
            }
            CallableImpl::Spec(spec_impl) if is_test_callable => {
                // Treat this as an intrinsic callable.
                self.analyze_intrinsic_callable();
                // Additionally, mark all the statements in every specialization such that they appear visited.
                self.set_all_stmts_in_block_to_default(spec_impl.body.block);
                spec_impl.adj.iter().for_each(|adj_impl| {
                    self.set_all_stmts_in_block_to_default(adj_impl.block);
                });
                spec_impl.ctl.iter().for_each(|ctl_impl| {
                    self.set_all_stmts_in_block_to_default(ctl_impl.block);
                });
                spec_impl.ctl_adj.iter().for_each(|ctladj_impl| {
                    self.set_all_stmts_in_block_to_default(ctladj_impl.block);
                });
            }
            CallableImpl::Spec(spec_impl) => {
                self.visit_spec_impl(spec_impl);
            }
        }
    }

    fn visit_expr(&mut self, expr_id: ExprId) {
        let expr = self.get_expr(expr_id);
        let mut compute_kind = match &expr.kind {
            ExprKind::Array(exprs) | ExprKind::ArrayLit(exprs) => self.analyze_expr_array(exprs),
            ExprKind::ArrayRepeat(value_expr_id, size_expr_id) => {
                self.analyze_expr_array_repeat(*value_expr_id, *size_expr_id)
            }
            ExprKind::Assign(assignee_expr_id, value_expr_id)
            | ExprKind::AssignField(assignee_expr_id, _, value_expr_id)
            | ExprKind::AssignOp(_, assignee_expr_id, value_expr_id) => {
                self.analyze_expr_assign(*assignee_expr_id, *value_expr_id)
            }
            ExprKind::AssignIndex(array_expr_id, index_expr_id, replacement_value_expr_id) => self
                .analyze_expr_assign_index(
                    *array_expr_id,
                    *index_expr_id,
                    *replacement_value_expr_id,
                ),
            ExprKind::BinOp(BinOp::Exp, lhs_expr_id, rhs_expr_id) => {
                self.analyze_expr_bin_op_exp(*lhs_expr_id, *rhs_expr_id)
            }
            ExprKind::BinOp(_, lhs_expr_id, rhs_expr_id) => {
                self.analyze_expr_bin_op(*lhs_expr_id, *rhs_expr_id, &expr.ty)
            }
            ExprKind::Block(block_id) => self.analyze_expr_block(*block_id),
            ExprKind::Call(callee_expr_id, args_expr_id) => {
                self.analyze_expr_call(*callee_expr_id, *args_expr_id, &expr.ty)
            }
            ExprKind::Closure(..) => ComputeKind::Classical,
            ExprKind::Fail(msg_expr_id) => self.analyze_expr_fail(*msg_expr_id),
            ExprKind::Field(record_expr_id, _) => {
                self.analyze_expr_field(*record_expr_id, &expr.ty)
            }
            ExprKind::Hole | ExprKind::Lit(_) => {
                // Hole and literal expressions are purely classical.
                ComputeKind::Classical
            }
            ExprKind::If(condition_expr_id, body_expr_id, otherwise_expr_id) => {
                let expr = self.get_expr(expr_id);
                self.analyze_expr_if(
                    *condition_expr_id,
                    *body_expr_id,
                    otherwise_expr_id.to_owned(),
                    &expr.ty,
                )
            }
            ExprKind::Index(array_expr_id, index_expr_id) => {
                self.analyze_expr_index(*array_expr_id, *index_expr_id, &expr.ty)
            }
            ExprKind::Range(start_expr_id, step_expr_id, end_expr_id) => self.analyze_expr_range(
                start_expr_id.to_owned(),
                step_expr_id.to_owned(),
                end_expr_id.to_owned(),
                &expr.ty,
            ),
            ExprKind::Return(value_expr_id) => {
                let compute_kind = self.analyze_expr_return(*value_expr_id);

                // Track the return expression at the application instance level.
                let application_instance = self.get_current_application_instance_mut();
                application_instance
                    .return_expressions
                    .push((expr_id, *value_expr_id));
                compute_kind
            }
            ExprKind::Struct(_, copy, fields) => self.analyze_expr_struct(*copy, fields, &expr.ty),
            ExprKind::String(components) => self.analyze_expr_string(components),
            ExprKind::Tuple(exprs) => self.analyze_expr_tuple(exprs),
            ExprKind::UnOp(_, operand_expr_id) => self.analyze_expr_un_op(*operand_expr_id),
            ExprKind::UpdateField(record_expr_id, _, replace_expr_id) => {
                self.analyze_expr_update_field(*record_expr_id, *replace_expr_id)
            }
            ExprKind::UpdateIndex(array_expr_id, index_expr_id, replacement_value_expr_id) => self
                .analyze_expr_update_index(
                    *array_expr_id,
                    *index_expr_id,
                    *replacement_value_expr_id,
                ),
            ExprKind::Var(res, _) => self.analyze_expr_var(res),
            ExprKind::While(condition_expr_id, block_id) => {
                self.analyze_expr_while(*condition_expr_id, *block_id)
            }
        };

        // If the expression's compute kind is of the quantum variant, then we need to do a couple more things to get
        // the final compute kind for the expression.
        if let ComputeKind::Quantum(quantum_properties) = &mut compute_kind {
            // Since the value kind does not handle all type structures (e.g. it does not handle the structure of a
            // tuple type), there could be a mismatch between the expected value kind variant for the expression's type
            // and the value kind that we got.
            // We fix this mismatch here.
            let mut value_kind = ValueKind::new_static_from_type(&expr.ty);
            quantum_properties
                .value_kind
                .project_onto_variant(&mut value_kind);
            quantum_properties.value_kind = value_kind;
        }

        // Finally, insert the expression's compute kind in the application instance.
        let application_instance = self.get_current_application_instance_mut();
        application_instance.insert_expr_compute_kind(expr_id, compute_kind);
    }

    fn visit_item(&mut self, item: &'a Item) {
        let current_item_context = self.get_current_item_context();
        match &item.kind {
            ItemKind::Callable(decl) => {
                self.visit_callable_decl(decl);
            }
            ItemKind::Export(_, _) | ItemKind::Namespace(_, _) | ItemKind::Ty(_, _) => {
                // Items that are not callables do not have compute properties by themselves so we just record them as
                // such in the package store compute properties data structure.
                self.package_store_compute_properties.insert_item(
                    current_item_context.id,
                    InternalItemComputeProperties::NonCallable,
                );
            }
        }
    }

    fn visit_package(&mut self, _: &'a Package, _: &PackageStore) {
        // Should never be called.
        unimplemented!("should never be called");
    }

    fn visit_pat(&mut self, _: PatId) {
        // Do nothing.
    }

    fn visit_spec_decl(&mut self, decl: &'a SpecDecl) {
        // Determine the compute properties of the specialization by visiting the implementation block for each
        // application variant.
        let mut are_variants_remaining = true;
        while are_variants_remaining {
            self.visit_block(decl.block);
            are_variants_remaining = self
                .get_current_spec_context_mut()
                .builder
                .advance_current_application_instance();
        }
    }

    fn visit_spec_impl(&mut self, spec_impl: &'a SpecImpl) {
        self.analyze_spec_decl(&spec_impl.body, FunctorSetValue::Empty);
        spec_impl
            .adj
            .iter()
            .for_each(|spec_decl| self.analyze_spec_decl(spec_decl, FunctorSetValue::Adj));
        spec_impl
            .ctl
            .iter()
            .for_each(|spec_decl| self.analyze_spec_decl(spec_decl, FunctorSetValue::Ctl));
        spec_impl
            .ctl_adj
            .iter()
            .for_each(|spec_decl| self.analyze_spec_decl(spec_decl, FunctorSetValue::CtlAdj));
    }

    fn visit_stmt(&mut self, stmt_id: StmtId) {
        let stmt = self.get_stmt(stmt_id);
        let compute_kind = match &stmt.kind {
            StmtKind::Expr(expr_id) => {
                // Visit the expression to determine its compute kind.
                self.visit_expr(*expr_id);

                // The statement's compute kind is the same as the expression's compute kind.
                let application_instance = self.get_current_application_instance();
                *application_instance.get_expr_compute_kind(*expr_id)
            }
            StmtKind::Semi(expr_id) => {
                // Visit the expression to determine its compute kind.
                self.visit_expr(*expr_id);

                // Use the expression compute kind to construct the statement compute kind, using only the expression
                // runtime features since the value kind is meaningless for semicolon statements.
                let application_instance = self.get_current_application_instance();
                let expr_compute_kind = *application_instance.get_expr_compute_kind(*expr_id);
                ComputeKind::Classical.aggregate_runtime_features(
                    expr_compute_kind,
                    ValueKind::Element(RuntimeKind::Static),
                )
            }
            StmtKind::Local(mutability, pat_id, value_expr_id) => {
                // Visit the expression to determine its compute kind.
                self.visit_expr(*value_expr_id);

                // Bind the expression's compute kind to the pattern.
                self.bind_expr_compute_kind_to_pattern(*mutability, *pat_id, *value_expr_id);

                // Use the expression compute kind to construct the statement compute kind, using only the expression
                // runtime features since the value kind is meaningless for local (binding) statements.
                let application_instance = self.get_current_application_instance();
                let expr_compute_kind = *application_instance.get_expr_compute_kind(*value_expr_id);
                ComputeKind::Classical.aggregate_runtime_features(
                    expr_compute_kind,
                    ValueKind::Element(RuntimeKind::Static),
                )
            }
            StmtKind::Item(_) => {
                // An item statement does not have any inherent quantum properties, so we just treat it as classical compute.
                ComputeKind::Classical
            }
        };

        // Insert the statements's compute kind into the application instance.
        let application_instance = self.get_current_application_instance_mut();
        application_instance.insert_stmt_compute_kind(stmt_id, compute_kind);
    }
}

#[allow(clippy::large_enum_variant)]
enum AnalysisContext {
    TopLevel(TopLevelContext),
    Item(ItemContext),
}

impl AnalysisContext {
    pub fn get_current_application_instance(&self) -> &ApplicationInstance {
        match self {
            Self::Item(item_context) => item_context.get_current_application_instance(),
            Self::TopLevel(top_level_context) => {
                top_level_context.get_current_application_instance()
            }
        }
    }

    pub fn get_current_application_instance_mut(&mut self) -> &mut ApplicationInstance {
        match self {
            Self::Item(item_context) => item_context.get_current_application_instance_mut(),
            Self::TopLevel(top_level_context) => {
                top_level_context.get_current_application_instance_mut()
            }
        }
    }
}

struct TopLevelContext {
    pub package_id: PackageId,
    builder: GeneratorSetsBuilder,
}

impl TopLevelContext {
    fn new(package_id: PackageId) -> Self {
        // A top-level context uses a generator sets builder that behaves like a parameterless callable of unit type.
        let builder = GeneratorSetsBuilder::new(&Vec::new(), None, &Ty::UNIT);
        Self {
            package_id,
            builder,
        }
    }

    pub fn get_current_application_instance(&self) -> &ApplicationInstance {
        self.builder.get_current_application_instance()
    }

    pub fn get_current_application_instance_mut(&mut self) -> &mut ApplicationInstance {
        self.builder.get_current_application_instance_mut()
    }
}

struct ItemContext {
    pub id: StoreItemId,
    callable_context: Option<CallableContext>,
    current_spec_context: Option<SpecContext>,
}

impl ItemContext {
    pub fn new(id: StoreItemId) -> Self {
        Self {
            id,
            callable_context: None,
            current_spec_context: None,
        }
    }

    pub fn clear_current_spec_context(&mut self) -> SpecContext {
        self.current_spec_context
            .take()
            .expect("current specialization context has already been cleared")
    }

    pub fn get_current_application_instance(&self) -> &ApplicationInstance {
        self.get_current_spec_context()
            .builder
            .get_current_application_instance()
    }

    pub fn get_current_application_instance_mut(&mut self) -> &mut ApplicationInstance {
        self.get_current_spec_context_mut()
            .builder
            .get_current_application_instance_mut()
    }

    pub fn get_current_spec_context(&self) -> &SpecContext {
        self.current_spec_context
            .as_ref()
            .expect("current specialization context is not set")
    }

    pub fn get_current_spec_context_mut(&mut self) -> &mut SpecContext {
        self.current_spec_context
            .as_mut()
            .expect("current specialization context is not set")
    }

    pub fn get_callable_context(&self) -> &CallableContext {
        self.callable_context
            .as_ref()
            .expect("callable declaration context should not be none")
    }

    pub fn get_input_params(&self) -> &Vec<InputParam> {
        &self.get_callable_context().input_params
    }

    pub fn set_callable_context(
        &mut self,
        kind: CallableKind,
        input_params: Vec<InputParam>,
        output_type: Ty,
        attrs: &[Attr],
    ) {
        assert!(self.callable_context.is_none());
        self.callable_context = Some(CallableContext {
            kind,
            input_params,
            output_type,
            attrs: attrs.to_vec(),
        });
    }

    pub fn set_current_spec_context(&mut self, spec_context: SpecContext) {
        assert!(self.current_spec_context.is_none());
        self.current_spec_context = Some(spec_context);
    }
}

struct CallableContext {
    pub kind: CallableKind,
    pub input_params: Vec<InputParam>,
    pub output_type: Ty,
    pub attrs: Vec<Attr>,
}

struct SpecContext {
    functor_set_value: FunctorSetValue,
    builder: GeneratorSetsBuilder,
}

impl SpecContext {
    pub fn new(
        functor_set_value: FunctorSetValue,
        input_params: &Vec<InputParam>,
        controls: Option<&Local>,
        return_type: &Ty,
    ) -> Self {
        let builder = GeneratorSetsBuilder::new(input_params, controls, return_type);
        Self {
            functor_set_value,
            builder,
        }
    }
}

enum CallComputeKind {
    Regular(ComputeKind),
    Override(ComputeKind),
}

fn derive_intrinsic_function_application_generator_set(
    callable_context: &CallableContext,
) -> ApplicationGeneratorSet {
    assert!(matches!(callable_context.kind, CallableKind::Function));

    // Determine the compute kind for all dynamic parameter applications.
    let mut dynamic_param_applications =
        Vec::<ParamApplication>::with_capacity(callable_context.input_params.len());
    for param in &callable_context.input_params {
        // For intrinsic functions, we assume any parameter can contribute to the output, so if any parameter is dynamic
        // the output of the function is dynamic.
        // When a parameter is bound to a dynamic value, its type contributes to the runtime features used by the
        // function application.
        let runtime_features = derive_runtime_features_for_value_kind_associated_to_type(
            ValueKind::new_dynamic_from_type(&param.ty),
            &param.ty,
        );
        let value_kind = ValueKind::new_dynamic_from_type(&callable_context.output_type);
        let param_compute_kind = ComputeKind::Quantum(QuantumProperties {
            runtime_features,
            value_kind,
        });

        // Create a parameter application depending on the parameter type.
        let param_application = match &param.ty {
            Ty::Array(_) => {
                array_param_application_from_runtime_features(runtime_features, value_kind)
            }
            _ => ParamApplication::Element(param_compute_kind),
        };
        dynamic_param_applications.push(param_application);
    }

    ApplicationGeneratorSet {
        // Functions are inherently classical.
        inherent: ComputeKind::Classical,
        dynamic_param_applications,
    }
}

fn array_param_application_from_runtime_features(
    runtime_features: RuntimeFeatureFlags,
    value_kind: ValueKind,
) -> ParamApplication {
    ParamApplication::Array(ArrayParamApplication {
        static_content_dynamic_size: ComputeKind::Quantum(QuantumProperties {
            runtime_features: runtime_features | RuntimeFeatureFlags::UseOfDynamicallySizedArray,
            value_kind,
        }),
        dynamic_content_static_size: ComputeKind::Quantum(QuantumProperties {
            runtime_features,
            value_kind,
        }),
        dynamic_content_dynamic_size: ComputeKind::Quantum(QuantumProperties {
            runtime_features: runtime_features | RuntimeFeatureFlags::UseOfDynamicallySizedArray,
            value_kind,
        }),
    })
}

fn derive_instrinsic_operation_application_generator_set(
    callable_context: &CallableContext,
) -> ApplicationGeneratorSet {
    assert!(matches!(callable_context.kind, CallableKind::Operation));

    // The value kind of intrinsic operations is inherently dynamic if their output is not `Unit` or `Qubit`.
    let value_kind = if callable_context.output_type == Ty::UNIT
        || callable_context.output_type == Ty::Prim(Prim::Qubit)
    {
        ValueKind::Element(RuntimeKind::Static)
    } else {
        ValueKind::new_dynamic_from_type(&callable_context.output_type)
    };

    let mut inherent_runtime_features = RuntimeFeatureFlags::empty();
    if callable_context.attrs.contains(&Attr::Measurement) {
        inherent_runtime_features |= RuntimeFeatureFlags::CallToCustomMeasurement;
    }
    if callable_context.attrs.contains(&Attr::Reset) {
        inherent_runtime_features |= RuntimeFeatureFlags::CallToCustomReset;
    }

    // The compute kind of intrinsic operations is always quantum.
    let inherent_compute_kind = ComputeKind::Quantum(QuantumProperties {
        runtime_features: inherent_runtime_features,
        value_kind,
    });

    // Determine the compute kind of all dynamic parameter applications.
    let mut dynamic_param_applications =
        Vec::<ParamApplication>::with_capacity(callable_context.input_params.len());
    for param in &callable_context.input_params {
        // For intrinsic operations, we assume any parameter can contribute to the output, so if any parameter is
        // dynamic the output of the operation is dynamic.
        // When a parameter is bound to a dynamic value, its type contributes to the runtime features used by the
        // operation application.
        let runtime_features = derive_runtime_features_for_value_kind_associated_to_type(
            ValueKind::new_dynamic_from_type(&param.ty),
            &param.ty,
        );
        let value_kind = ValueKind::new_dynamic_from_type(&callable_context.output_type);
        let param_compute_kind = ComputeKind::Quantum(QuantumProperties {
            runtime_features,
            value_kind,
        });

        // Create a parameter application depending on the parameter type.
        let param_application = match &param.ty {
            Ty::Array(_) => {
                array_param_application_from_runtime_features(runtime_features, value_kind)
            }
            _ => ParamApplication::Element(param_compute_kind),
        };
        dynamic_param_applications.push(param_application);
    }

    ApplicationGeneratorSet {
        inherent: inherent_compute_kind,
        dynamic_param_applications,
    }
}

fn is_length_intrinsic(callable_decl: &CallableDecl) -> bool {
    matches!(callable_decl.implementation, CallableImpl::Intrinsic)
        && callable_decl.name.name.as_ref() == "Length"
}

fn ty_to_runtime_runtime_output_flags(ty: &Ty) -> RuntimeFeatureFlags {
    match ty {
        Ty::Array(content_type) => ty_to_runtime_runtime_output_flags(content_type),
        Ty::Prim(prim) => ty_prim_to_runtime_output_flag(*prim),
        Ty::Tuple(element_types) => {
            let mut runtime_features = RuntimeFeatureFlags::empty();
            for element_type in element_types {
                let element_runtime_features = ty_to_runtime_runtime_output_flags(element_type);
                runtime_features |= element_runtime_features;
            }
            runtime_features
        }
        Ty::Arrow(_) | Ty::Udt(_) => RuntimeFeatureFlags::UseOfAdvancedOutput,
        Ty::Infer(_) => panic!("cannot derive runtime features for `Infer` type"),
        Ty::Param(_) => panic!("cannot derive runtime features for `Param` type"),
        Ty::Err => panic!("cannot derive runtime features for `Err` type"),
    }
}

fn ty_prim_to_runtime_output_flag(prim: Prim) -> RuntimeFeatureFlags {
    match prim {
        Prim::Bool => RuntimeFeatureFlags::UseOfBoolOutput,
        Prim::Double => RuntimeFeatureFlags::UseOfDoubleOutput,
        Prim::Int => RuntimeFeatureFlags::UseOfIntOutput,
        Prim::Result => RuntimeFeatureFlags::empty(),
        Prim::BigInt
        | Prim::Pauli
        | Prim::Qubit
        | Prim::Range
        | Prim::RangeFrom
        | Prim::RangeTo
        | Prim::RangeFull
        | Prim::String => RuntimeFeatureFlags::UseOfAdvancedOutput,
    }
}

#[allow(clippy::too_many_lines)]
fn derive_runtime_features_for_value_kind_associated_to_type(
    value_kind: ValueKind,
    ty: &Ty,
) -> RuntimeFeatureFlags {
    fn derive_runtime_features_for_value_kind_associated_to_array(
        value_kind: ValueKind,
        content_type: &Ty,
    ) -> RuntimeFeatureFlags {
        let ValueKind::Array(content_runtime_kind, size_runtime_kind) = value_kind else {
            panic!("expected array variant of value kind");
        };

        let mut runtime_features = RuntimeFeatureFlags::empty();

        // A dynamic array is dynamically sized.
        if matches!(size_runtime_kind, RuntimeKind::Dynamic) {
            runtime_features |= RuntimeFeatureFlags::UseOfDynamicallySizedArray;
        }

        // A dynamic array has dynamic content so we need to include the runtime features used by its content.
        if matches!(content_runtime_kind, RuntimeKind::Dynamic) {
            let content_value_kind = ValueKind::new_dynamic_from_type(content_type);
            runtime_features |= derive_runtime_features_for_value_kind_associated_to_type(
                content_value_kind,
                content_type,
            );
        }

        runtime_features
    }

    fn derive_runtime_features_for_value_kind_associated_to_arrow(
        value_kind: ValueKind,
        arrow: &Arrow,
    ) -> RuntimeFeatureFlags {
        let ValueKind::Element(runtime_kind) = value_kind else {
            panic!("expected element variant of value kind");
        };

        if matches!(runtime_kind, RuntimeKind::Static) {
            return RuntimeFeatureFlags::empty();
        }

        match arrow.kind {
            CallableKind::Function => RuntimeFeatureFlags::UseOfDynamicArrowFunction,
            CallableKind::Operation => RuntimeFeatureFlags::UseOfDynamicArrowOperation,
        }
    }

    fn derive_runtime_features_for_value_kind_associated_to_primitive_type(
        value_kind: ValueKind,
        prim: Prim,
    ) -> RuntimeFeatureFlags {
        match value_kind {
            ValueKind::Array(RuntimeKind::Static, RuntimeKind::Static)
            | ValueKind::Element(RuntimeKind::Static) => {
                return RuntimeFeatureFlags::empty();
            }
            _ => (),
        }

        match prim {
            Prim::BigInt => RuntimeFeatureFlags::UseOfDynamicBigInt,
            Prim::Bool => RuntimeFeatureFlags::UseOfDynamicBool,
            Prim::Double => RuntimeFeatureFlags::UseOfDynamicDouble,
            Prim::Int => RuntimeFeatureFlags::UseOfDynamicInt,
            Prim::Pauli => RuntimeFeatureFlags::UseOfDynamicPauli,
            Prim::Qubit => RuntimeFeatureFlags::UseOfDynamicQubit,
            Prim::Range | Prim::RangeFrom | Prim::RangeTo | Prim::RangeFull => {
                RuntimeFeatureFlags::UseOfDynamicRange
            }
            // Results are inherently dynamic but they do not need special runtime features just to exist.
            Prim::Result => RuntimeFeatureFlags::empty(),
            Prim::String => RuntimeFeatureFlags::UseOfDynamicString,
        }
    }

    fn derive_runtime_features_for_value_kind_associated_to_primitive_tuple(
        value_kind: ValueKind,
        element_types: &Vec<Ty>,
    ) -> RuntimeFeatureFlags {
        let ValueKind::Element(runtime_kind) = value_kind else {
            panic!("expected element variant of value kind");
        };

        if matches!(runtime_kind, RuntimeKind::Static) {
            return RuntimeFeatureFlags::empty();
        }

        let mut runtime_features = RuntimeFeatureFlags::empty();
        for element_type in element_types {
            let element_value_kind = ValueKind::new_dynamic_from_type(element_type);
            runtime_features |= derive_runtime_features_for_value_kind_associated_to_type(
                element_value_kind,
                element_type,
            );
        }
        runtime_features
    }

    fn derive_runtime_features_for_value_kind_associated_to_udt(
        value_kind: ValueKind,
    ) -> RuntimeFeatureFlags {
        let ValueKind::Element(runtime_kind) = value_kind else {
            panic!("expected element variant of value kind");
        };

        match runtime_kind {
            RuntimeKind::Dynamic => RuntimeFeatureFlags::UseOfDynamicUdt,
            RuntimeKind::Static => RuntimeFeatureFlags::empty(),
        }
    }

    match ty {
        Ty::Array(content_type) => {
            derive_runtime_features_for_value_kind_associated_to_array(value_kind, content_type)
        }
        Ty::Arrow(arrow) => {
            derive_runtime_features_for_value_kind_associated_to_arrow(value_kind, arrow)
        }
        Ty::Infer(_) => panic!("cannot derive runtime features for `Infer` type"),
        // Generic types do not require additional runtime features.
        Ty::Param(_) => RuntimeFeatureFlags::empty(),
        Ty::Prim(prim) => {
            derive_runtime_features_for_value_kind_associated_to_primitive_type(value_kind, *prim)
        }
        Ty::Tuple(element_types) => {
            derive_runtime_features_for_value_kind_associated_to_primitive_tuple(
                value_kind,
                element_types,
            )
        }
        Ty::Udt(_) => derive_runtime_features_for_value_kind_associated_to_udt(value_kind),
        Ty::Err => panic!("cannot derive runtime features for `Err` type"),
    }
}

fn derive_specialization_controls(
    spec_decl: &SpecDecl,
    pats: &IndexMap<PatId, Pat>,
) -> Option<Local> {
    spec_decl.input.and_then(|pat_id| {
        let pat = pats.get(pat_id).expect("pat should exist");
        match &pat.kind {
            PatKind::Bind(ident) => Some(Local {
                var: ident.id,
                kind: LocalKind::SpecInput,
            }),
            PatKind::Discard => None, // Nothing to bind to.
            PatKind::Tuple(_) => panic!("expected specialization input pattern"),
        }
    })
}

/// Maps an input pattern to a list of expressions that correspond to identifiers or discards.
fn map_input_pattern_to_input_expressions(
    pat_id: StorePatId,
    expr_id: StoreExprId,
    package_store: &impl PackageStoreLookup,
    skip_ahead: Option<usize>,
) -> Vec<ExprId> {
    let pat = package_store.get_pat(pat_id);
    match &pat.kind {
        PatKind::Bind(_) | PatKind::Discard => vec![expr_id.expr],
        PatKind::Tuple(pats) => {
            // Map each one of the elements in the pattern to an expression in the tuple.
            let pats = &pats[skip_ahead.unwrap_or_default()..];
            let expr = package_store.get_expr(expr_id);
            if let ExprKind::Tuple(exprs) = &expr.kind {
                let pats = if skip_ahead.is_some() && exprs.len() > 1 {
                    // When skip_ahead is not None we know we are processing a lambda, so the final pattern is itself a tuple.
                    // Unpack the tuple pattern to get the individual elements and map input to those.
                    let PatKind::Tuple(pats) =
                        &package_store.get_pat((pat_id.package, pats[0]).into()).kind
                    else {
                        panic!("expected tuple pattern for lambda receiving multiple arguments");
                    };
                    pats
                } else {
                    pats
                };
                let mut input_param_exprs = Vec::<ExprId>::with_capacity(pats.len());
                for (local_pat_id, local_expr_id) in pats.iter().zip(exprs.iter()) {
                    let global_pat_id = StorePatId::from((pat_id.package, *local_pat_id));
                    let global_expr_id = StoreExprId::from((expr_id.package, *local_expr_id));
                    let mut sub_input_param_exprs = map_input_pattern_to_input_expressions(
                        global_pat_id,
                        global_expr_id,
                        package_store,
                        None,
                    );
                    input_param_exprs.append(&mut sub_input_param_exprs);
                }
                input_param_exprs
            } else {
                // All elements in the pattern map to the same expression.
                // This is one of the boundaries where we can lose specific information since we are "unpacking" the
                // tuple represented by a single expression.
                let pats_len = if skip_ahead.is_some() {
                    // When skip_ahead is not None we know we are processing a lambda, so check if pattern is itself a tuple.
                    // If it is, that is the length we need to use rather than the original one.
                    if let PatKind::Tuple(pats) =
                        &package_store.get_pat((pat_id.package, pats[0]).into()).kind
                    {
                        pats.len()
                    } else {
                        pats.len()
                    }
                } else {
                    pats.len()
                };

                vec![expr_id.expr; pats_len]
            }
        }
    }
}

fn split_controls_and_input(
    args_expr_id: ExprId,
    functor_app: FunctorApp,
    package: &impl PackageLookup,
) -> (Vec<ExprId>, ExprId) {
    let mut controls = Vec::new();
    let mut remainder_expr_id = args_expr_id;
    for _ in 0..functor_app.controlled {
        let expr = package.get_expr(remainder_expr_id);
        let ExprKind::Tuple(pats) = &expr.kind else {
            panic!("expected tuple expression");
        };
        assert!(pats.len() == 2);
        controls.push(pats[0]);
        remainder_expr_id = pats[1];
    }
    (controls, remainder_expr_id)
}

fn is_any_result(t: &Ty) -> bool {
    match t {
        Ty::Prim(Prim::Result) => true,
        Ty::Array(t) => is_any_result(t),
        Ty::Tuple(ts) => ts.iter().any(is_any_result),
        _ => false,
    }
}
