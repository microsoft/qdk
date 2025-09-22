// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    ApplicationGeneratorSet, ComputeKind, QuantumProperties, RuntimeFeatureFlags, RuntimeKind,
    ValueKind,
    common::{Local, LocalKind, LocalsLookup, initialize_locals_map},
    scaffolding::InternalPackageComputeProperties,
};
use qsc_data_structures::index_map::IndexMap;
use qsc_fir::{
    extensions::{InputParam, InputParamIndex},
    fir::{BlockId, ExprId, LocalVarId, StmtId},
    ty::Ty,
};
use rustc_hash::FxHashMap;

/// Auxiliary data structure used to build multiple related application generator sets from individual application
/// instances for a particular callable specialization.
#[derive(Debug)]
pub struct GeneratorSetsBuilder {
    application_instances: Vec<Vec<ApplicationInstance>>,
    current_application_instance: usize,
    input_params_count: usize,
}

impl GeneratorSetsBuilder {
    /// Creates a new builder.
    pub fn new(input_params: &Vec<InputParam>, controls: Option<&Local>, return_type: &Ty) -> Self {
        // Create a vector of vectors of application instances.
        // - The first element in the top-level vector represents the inherent variant. It always is a single-element
        //   vector.
        // - Each of the other elements in the top-level vector represent the variant(s) of each dynamic parameter
        //   application. For array parameters it is a three-element vector and for the parameters of any other type it
        //   is a single-element vector.
        let mut application_instances =
            Vec::<Vec<ApplicationInstance>>::with_capacity(input_params.len() + 1);

        // Insert the inherent application instance.
        application_instances.push(vec![ApplicationInstance::new(
            input_params,
            controls,
            return_type,
            None,
        )]);

        // Insert the application instances representing the dynamic variant(s) of each input parameter.
        for input_param in input_params {
            let input_param_variants = match input_param.ty {
                Ty::Array(_) => {
                    // For parameters of type array, three dynamic variants exit. Each
                    // - An array with static content and dynamic size.
                    // - An array with dynamic content and static size.
                    // - An array with dynamic content and dynamic size.
                    let static_content_dynamic_size = ApplicationInstance::new(
                        input_params,
                        controls,
                        return_type,
                        Some((
                            input_param.index,
                            ValueKind::Array(RuntimeKind::Static, RuntimeKind::Dynamic),
                        )),
                    );
                    let dynamic_content_static_size = ApplicationInstance::new(
                        input_params,
                        controls,
                        return_type,
                        Some((
                            input_param.index,
                            ValueKind::Array(RuntimeKind::Dynamic, RuntimeKind::Static),
                        )),
                    );
                    let dynamic_content_dynamic_size = ApplicationInstance::new(
                        input_params,
                        controls,
                        return_type,
                        Some((
                            input_param.index,
                            ValueKind::Array(RuntimeKind::Dynamic, RuntimeKind::Dynamic),
                        )),
                    );
                    vec![
                        static_content_dynamic_size,
                        dynamic_content_static_size,
                        dynamic_content_dynamic_size,
                    ]
                }
                _ => {
                    // For non-array params, only one dynamic variant exists.
                    vec![ApplicationInstance::new(
                        input_params,
                        controls,
                        return_type,
                        Some((input_param.index, ValueKind::Element(RuntimeKind::Dynamic))),
                    )]
                }
            };
            application_instances.push(input_param_variants);
        }

        Self {
            application_instances,
            current_application_instance: 0,
            input_params_count: input_params.len(),
        }
    }

    pub fn advance_current_application_instance(&mut self) -> bool {
        if self.current_application_instance < self.flattened_application_instances().count() - 1 {
            self.current_application_instance += 1;
            return true;
        }

        false
    }

    pub fn get_current_application_instance(&self) -> &ApplicationInstance {
        let current_application_instance = self.current_application_instance;
        self.flattened_application_instances()
            .nth(current_application_instance)
            .expect("nth application instace does not exist")
    }

    pub fn get_current_application_instance_mut(&mut self) -> &mut ApplicationInstance {
        let current_application_instance = self.current_application_instance;
        self.flattened_application_instances_mut()
            .nth(current_application_instance)
            .expect("nth application instance does not exist")
    }

    /// Saves the contents of the builder to the package compute properties data structure.
    /// If a main block ID is provided, it returns the applications generator set representing the block.
    pub fn save_to_package_compute_properties(
        mut self,
        package_compute_properties: &mut InternalPackageComputeProperties,
        main_block: Option<BlockId>,
    ) -> Option<ApplicationGeneratorSet> {
        // Get the compute properties of the inherent application instance and the non-static parameter applications.
        let mut inherent_application_compute_properties = self.close_inherent();

        // Get the compute properties of each parameter application.
        let mut dynamic_param_applications_compute_properties =
            Vec::<ParamApplicationComputeProperties>::with_capacity(self.input_params_count);
        for input_param_index in (0..self.input_params_count).map(InputParamIndex::from) {
            let param_application_compute_properties = self.close_param(input_param_index);
            dynamic_param_applications_compute_properties
                .push(param_application_compute_properties);
        }

        // Save the compute properties to the package.
        Self::save_application_generator_sets(
            &mut inherent_application_compute_properties,
            &mut dynamic_param_applications_compute_properties,
            package_compute_properties,
        );

        // If a main block was provided, create an applications generator that represents the specialization based on
        // the applications generator of the main block.

        main_block.map(|main_block_id| {
            let mut applications_generator = package_compute_properties
                .blocks
                .get(main_block_id)
                .expect("block applications generator should exist")
                .clone();
            assert!(
                applications_generator.dynamic_param_applications.len()
                    == dynamic_param_applications_compute_properties.len()
            );

            // Update the value kind of the generator's inherent compute kind.
            if let Some(inherent_value_kind) = inherent_application_compute_properties.value_kind {
                applications_generator
                    .inherent
                    .aggregate_value_kind(inherent_value_kind);
            }

            // Update the value kind of the generator's param applications.
            for (param_application, compute_properties) in applications_generator
                .dynamic_param_applications
                .iter_mut()
                .zip(dynamic_param_applications_compute_properties)
            {
                Self::aggregate_param_application_value_kind(
                    param_application,
                    &compute_properties,
                );
            }

            // Return the applications gene with the updated dynamism sources.
            applications_generator
        })
    }

    fn aggregate_param_application_value_kind(
        param_application: &mut crate::ParamApplication,
        compute_properties: &ParamApplicationComputeProperties,
    ) {
        match param_application {
            crate::ParamApplication::Array(array_param_application) => {
                let ParamApplicationComputeProperties::Array(array_compute_properties) =
                    compute_properties
                else {
                    panic!("expected an array param application");
                };
                array_compute_properties
                    .static_content_dynamic_size
                    .value_kind
                    .iter()
                    .for_each(|value_kind| {
                        array_param_application
                            .static_content_dynamic_size
                            .aggregate_value_kind(*value_kind);
                    });
                array_compute_properties
                    .dynamic_content_static_size
                    .value_kind
                    .iter()
                    .for_each(|value_kind| {
                        array_param_application
                            .dynamic_content_static_size
                            .aggregate_value_kind(*value_kind);
                    });
                array_compute_properties
                    .dynamic_content_dynamic_size
                    .value_kind
                    .iter()
                    .for_each(|value_kind| {
                        array_param_application
                            .dynamic_content_dynamic_size
                            .aggregate_value_kind(*value_kind);
                    });
            }
            crate::ParamApplication::Element(element_param_application) => {
                let ParamApplicationComputeProperties::Element(element_compute_properties) =
                    compute_properties
                else {
                    panic!("expected an element param application");
                };
                element_compute_properties
                    .value_kind
                    .iter()
                    .for_each(|value_kind| {
                        element_param_application.aggregate_value_kind(*value_kind);
                    });
            }
        }
    }

    fn close_inherent(&mut self) -> ApplicationInstanceComputeProperties {
        // The inherent param application is always the first one.
        let mut variants = self.application_instances[0].drain(..).collect::<Vec<_>>();
        let inherent_application_instance = variants
            .pop()
            .expect("inherent application instance could not be popped");
        inherent_application_instance.close()
    }

    fn close_param(&mut self, param_index: InputParamIndex) -> ParamApplicationComputeProperties {
        const DYNAMIC_ELEMENTS_PARAM_VARIANTS: usize = 1;
        const DYNAMIC_ARRAY_PARAM_VARIANTS: usize = 3;

        // We need to offset the index since the first top-level vector in application instances is reserved for the
        // inherent variant.
        let variants_index = usize::from(param_index) + 1usize;
        let variants = &mut self.application_instances[variants_index]
            .drain(..)
            .collect::<Vec<_>>();

        // The kind of parameter application we create depends on the number of variants that the parameter has.
        if variants.len() == DYNAMIC_ELEMENTS_PARAM_VARIANTS {
            let application_instance = variants
                .pop()
                .expect("element parameter application instance could not be popped");
            let compute_properties = application_instance.close();
            ParamApplicationComputeProperties::Element(compute_properties)
        } else if variants.len() == DYNAMIC_ARRAY_PARAM_VARIANTS {
            // IMPORTANT: the poisition of each application instance in the variants vector has a specific meaning, so
            // we need the order of pops is consequential.
            let dynamic_content_dynamic_size_application_instance = variants
                .pop()
                .expect("array parameter application instance could not be popped");
            let dynamic_content_static_size_application_instance = variants
                .pop()
                .expect("array parameter application instance could not be popped");
            let static_content_dynamic_size_application_instance = variants
                .pop()
                .expect("array parameter application instance could not be popped");
            ParamApplicationComputeProperties::Array(Box::new(
                ArrayParamApplicationComputeProperties {
                    static_content_dynamic_size: static_content_dynamic_size_application_instance
                        .close(),
                    dynamic_content_static_size: dynamic_content_static_size_application_instance
                        .close(),
                    dynamic_content_dynamic_size: dynamic_content_dynamic_size_application_instance
                        .close(),
                },
            ))
        } else {
            panic!("invalid number of parameter application variants");
        }
    }

    fn flattened_application_instances(&self) -> impl Iterator<Item = &ApplicationInstance> {
        self.application_instances.iter().flat_map(|v| v.iter())
    }

    fn flattened_application_instances_mut(
        &mut self,
    ) -> impl Iterator<Item = &mut ApplicationInstance> {
        self.application_instances
            .iter_mut()
            .flat_map(|v| v.iter_mut())
    }

    fn save_application_generator_sets(
        inherent_application_compute_properties: &mut ApplicationInstanceComputeProperties,
        dynamic_param_applications_compute_properties: &mut [ParamApplicationComputeProperties],
        package_compute_properties: &mut InternalPackageComputeProperties,
    ) {
        let input_params_count = dynamic_param_applications_compute_properties.len();

        // Save an applications generator set for each block using their compute properties.
        for (block_id, block_inherent_compute_kind) in
            inherent_application_compute_properties.blocks.drain()
        {
            let mut block_dynamic_param_applications =
                Vec::<crate::ParamApplication>::with_capacity(input_params_count);
            for param_application_compute_properties in
                dynamic_param_applications_compute_properties.iter_mut()
            {
                let param_application = param_application_compute_properties
                    .remove_item(ApplicationInstanceItem::Block(block_id));
                block_dynamic_param_applications.push(param_application);
            }
            let application_generator_set = ApplicationGeneratorSet {
                inherent: block_inherent_compute_kind,
                dynamic_param_applications: block_dynamic_param_applications,
            };
            package_compute_properties
                .blocks
                .insert(block_id, application_generator_set);
        }

        // Save an applications generator set for each statement using their compute properties.
        for (stmt_id, stmt_inherent_compute_kind) in
            inherent_application_compute_properties.stmts.drain()
        {
            let mut stmt_dynamic_param_applications =
                Vec::<crate::ParamApplication>::with_capacity(input_params_count);
            for param_application_compute_properties in
                dynamic_param_applications_compute_properties.iter_mut()
            {
                let param_application = param_application_compute_properties
                    .remove_item(ApplicationInstanceItem::Stmt(stmt_id));
                stmt_dynamic_param_applications.push(param_application);
            }
            let application_generator_set = ApplicationGeneratorSet {
                inherent: stmt_inherent_compute_kind,
                dynamic_param_applications: stmt_dynamic_param_applications,
            };
            package_compute_properties
                .stmts
                .insert(stmt_id, application_generator_set);
        }

        // Save an applications generator set for each expression using their compute properties.
        for (expr_id, expr_inherent_compute_kind) in
            inherent_application_compute_properties.exprs.drain()
        {
            let mut expr_dynamic_param_applications =
                Vec::<crate::ParamApplication>::with_capacity(input_params_count);
            for param_application_compute_properties in
                dynamic_param_applications_compute_properties.iter_mut()
            {
                let param_application = param_application_compute_properties
                    .remove_item(ApplicationInstanceItem::Expr(expr_id));
                expr_dynamic_param_applications.push(param_application);
            }
            let application_generator_set = ApplicationGeneratorSet {
                inherent: expr_inherent_compute_kind,
                dynamic_param_applications: expr_dynamic_param_applications,
            };
            package_compute_properties
                .exprs
                .insert(expr_id, application_generator_set);
        }

        // Save the unresolved callee expressions.
        for unresolved_callee_expr_id in inherent_application_compute_properties
            .unresolved_callee_exprs
            .drain(..)
        {
            package_compute_properties
                .unresolved_callee_exprs
                .push(unresolved_callee_expr_id);
        }
    }
}

/// An instance of a callable application.
#[derive(Debug, Default)]
pub struct ApplicationInstance {
    /// A map of locals with their associated compute kind.
    pub locals_map: LocalsComputeKindMap,
    /// The currently active dynamic scopes in the application instance.
    pub active_dynamic_scopes: Vec<ExprId>,
    /// The return expressions throughout the application instance.
    /// The first ID in the tuple represents the return expression itself.
    /// The second ID in the tuple represents the returned value expression.
    pub return_expressions: Vec<(ExprId, ExprId)>,
    /// The return type of the application instance.
    return_type: Ty,
    /// The compute kind of the blocks related to the application instance.
    blocks: FxHashMap<BlockId, ComputeKind>,
    /// The compute kind of the statements related to the application instance.
    stmts: FxHashMap<StmtId, ComputeKind>,
    /// The compute kind of the expressions related to the application instance.
    exprs: FxHashMap<ExprId, ComputeKind>,
    pub unresolved_callee_exprs: Vec<ExprId>,
}

impl ApplicationInstance {
    pub fn find_block_compute_kind(&self, id: BlockId) -> Option<&ComputeKind> {
        self.blocks.get(&id)
    }

    pub fn find_expr_compute_kind(&self, id: ExprId) -> Option<&ComputeKind> {
        self.exprs.get(&id)
    }

    pub fn find_stmt_compute_kind(&self, id: StmtId) -> Option<&ComputeKind> {
        self.stmts.get(&id)
    }

    pub fn get_block_compute_kind(&self, id: BlockId) -> &ComputeKind {
        self.find_block_compute_kind(id)
            .expect("block compute kind should exist in application instance")
    }

    pub fn get_expr_compute_kind(&self, id: ExprId) -> &ComputeKind {
        self.find_expr_compute_kind(id)
            .expect("expression compute kind should exist in application instance")
    }

    pub fn get_stmt_compute_kind(&self, id: StmtId) -> &ComputeKind {
        self.find_stmt_compute_kind(id)
            .expect("expression compute kind should exist in application instance")
    }

    pub fn insert_block_compute_kind(&mut self, id: BlockId, value: ComputeKind) {
        self.blocks.insert(id, value);
    }

    pub fn insert_expr_compute_kind(&mut self, id: ExprId, value: ComputeKind) {
        self.exprs.insert(id, value);
    }

    pub fn insert_stmt_compute_kind(&mut self, id: StmtId, value: ComputeKind) {
        self.stmts.insert(id, value);
    }

    fn new(
        input_params: &Vec<InputParam>,
        controls: Option<&Local>,
        return_type: &Ty,
        dynamic_param: Option<(InputParamIndex, ValueKind)>,
    ) -> Self {
        // Initialize the locals map with the specialization controls (if any).
        let mut locals_map = LocalsComputeKindMap::default();
        if let Some(controls) = controls {
            // Controls compute properties are handled at the call expression, so just use quantum compute kind with
            // no runtime features here.
            let compute_kind = ComputeKind::Quantum(QuantumProperties {
                runtime_features: RuntimeFeatureFlags::empty(),
                value_kind: ValueKind::Array(RuntimeKind::Static, RuntimeKind::Static),
            });
            locals_map.insert(
                controls.var,
                LocalComputeKind {
                    local: controls.clone(),
                    compute_kind,
                },
            );
        }

        let mut unprocessed_locals_map = initialize_locals_map(input_params);
        for (local_var_id, local) in unprocessed_locals_map.drain() {
            let LocalKind::InputParam(input_param_index) = local.kind else {
                panic!("only input parameters are expected");
            };

            // If a dynamic application is provided, set the compute kind associated to the parameter accordingly.
            let mut compute_kind = ComputeKind::Classical;
            if let Some((dynamic_param_index, dynamic_param_value_kind)) = dynamic_param {
                if input_param_index == dynamic_param_index {
                    compute_kind = ComputeKind::Quantum(QuantumProperties {
                        runtime_features: RuntimeFeatureFlags::empty(),
                        value_kind: dynamic_param_value_kind,
                    });
                }
            }

            locals_map.insert(
                local_var_id,
                LocalComputeKind {
                    local,
                    compute_kind,
                },
            );
        }
        Self {
            locals_map,
            active_dynamic_scopes: Vec::new(),
            return_expressions: Vec::new(),
            return_type: return_type.clone(),
            blocks: FxHashMap::default(),
            stmts: FxHashMap::default(),
            exprs: FxHashMap::default(),
            unresolved_callee_exprs: Vec::new(),
        }
    }

    fn close(self) -> ApplicationInstanceComputeProperties {
        // Determine the value kind of the application instance by going through each return expression aggregating
        // their value kind (if any).
        let mut value_kinds = Vec::<ValueKind>::new();
        for (return_expr_id, returned_value_expr_id) in self.return_expressions.clone() {
            let return_expr_compute_kind = self.get_expr_compute_kind(return_expr_id);

            // There are two scenarios in which a value kind is considered, and both of them only happen if the return
            // expression is quantum.
            if let ComputeKind::Quantum(return_quantum_properties) = return_expr_compute_kind {
                let return_value_kind = if return_quantum_properties
                    .runtime_features
                    .contains(RuntimeFeatureFlags::ReturnWithinDynamicScope)
                {
                    // The return expression happens within a dynamic scope so the value kind is dynamic.
                    ValueKind::new_dynamic_from_type(&self.return_type)
                } else {
                    // What we actually want here is the value kind of the returned value expression.
                    let returned_value_expr_compute_kind =
                        self.get_expr_compute_kind(returned_value_expr_id);
                    let ComputeKind::Quantum(returned_value_quantum_properties) =
                        returned_value_expr_compute_kind
                    else {
                        panic!("returned value expression is expected to be quantum");
                    };
                    returned_value_quantum_properties.value_kind
                };
                value_kinds.push(return_value_kind);
            }
        }

        // An application instance does not always have a value kind, only when there is at least one quantum return
        // expression.
        let value_kind = if value_kinds.is_empty() {
            None
        } else {
            let initial_value_kind = if let Ty::Array(_) = self.return_type {
                ValueKind::Array(RuntimeKind::Static, RuntimeKind::Static)
            } else {
                ValueKind::Element(RuntimeKind::Static)
            };
            let value_kind = value_kinds.iter().fold(
                initial_value_kind,
                |aggregated_value_kind, return_value_kind| {
                    aggregated_value_kind.aggregate(*return_value_kind)
                },
            );
            Some(value_kind)
        };

        ApplicationInstanceComputeProperties {
            blocks: self.blocks,
            stmts: self.stmts,
            exprs: self.exprs,
            unresolved_callee_exprs: self.unresolved_callee_exprs,
            value_kind,
        }
    }
}

#[derive(Debug, Default)]
pub struct LocalsComputeKindMap(IndexMap<LocalVarId, LocalComputeKind>);

impl LocalsLookup for LocalsComputeKindMap {
    fn find(&self, local_var_id: LocalVarId) -> Option<&Local> {
        self.0
            .get(local_var_id)
            .map(|local_compute_kind| &local_compute_kind.local)
    }
}

impl LocalsComputeKindMap {
    pub fn aggregate_compute_kind(&mut self, local_var_id: LocalVarId, delta: ComputeKind) {
        let local_compute_kind = self
            .0
            .get_mut(local_var_id)
            .expect("local compute kind does not exist");
        local_compute_kind.compute_kind = local_compute_kind.compute_kind.aggregate(delta);
    }

    pub fn find_local_compute_kind(&self, local_var_id: LocalVarId) -> Option<&LocalComputeKind> {
        self.0.get(local_var_id)
    }

    pub fn get_or_init_local_compute_kind(
        &mut self,
        local_var_id: LocalVarId,
        local_kind: LocalKind,
        compute_kind: ComputeKind,
    ) -> &LocalComputeKind {
        if self.0.contains_key(local_var_id) {
            self.0.get(local_var_id).expect("local should exist")
        } else {
            self.0.insert(
                local_var_id,
                LocalComputeKind {
                    local: Local {
                        var: local_var_id,
                        kind: local_kind,
                    },
                    compute_kind,
                },
            );
            self.0.get(local_var_id).expect("local should exist")
        }
    }

    pub fn insert(&mut self, local_var_id: LocalVarId, value: LocalComputeKind) {
        self.0.insert(local_var_id, value);
    }
}

#[derive(Debug)]
pub struct LocalComputeKind {
    pub local: Local,
    pub compute_kind: ComputeKind,
}

#[derive(Clone, Copy)]
enum ApplicationInstanceItem {
    Block(BlockId),
    Expr(ExprId),
    Stmt(StmtId),
}

struct ApplicationInstanceComputeProperties {
    blocks: FxHashMap<BlockId, ComputeKind>,
    stmts: FxHashMap<StmtId, ComputeKind>,
    exprs: FxHashMap<ExprId, ComputeKind>,
    value_kind: Option<ValueKind>,
    unresolved_callee_exprs: Vec<ExprId>,
}

impl ApplicationInstanceComputeProperties {
    fn remove(&mut self, item: ApplicationInstanceItem) -> ComputeKind {
        match item {
            ApplicationInstanceItem::Block(block_id) => self.remove_block(block_id),
            ApplicationInstanceItem::Expr(expr_id) => self.remove_expr(expr_id),
            ApplicationInstanceItem::Stmt(stmt_id) => self.remove_stmt(stmt_id),
        }
    }

    fn remove_block(&mut self, id: BlockId) -> ComputeKind {
        self.blocks.remove(&id).expect(
            "block to be removed should exist in the compute properties of the application instance",
        )
    }

    fn remove_stmt(&mut self, id: StmtId) -> ComputeKind {
        self.stmts.remove(&id).expect(
            "statement to be removed should exist in the compute properties of the application instance",
        )
    }

    fn remove_expr(&mut self, id: ExprId) -> ComputeKind {
        self.exprs.remove(&id).expect(
            "expression to be removed should exist in the compute properties of the application instance",
        )
    }
}

enum ParamApplicationComputeProperties {
    Element(ApplicationInstanceComputeProperties),
    Array(Box<ArrayParamApplicationComputeProperties>),
}

impl ParamApplicationComputeProperties {
    fn remove_item(&mut self, item: ApplicationInstanceItem) -> crate::ParamApplication {
        match self {
            Self::Element(compute_properties) => {
                let compute_kind = compute_properties.remove(item);
                crate::ParamApplication::Element(compute_kind)
            }
            Self::Array(array_param) => {
                let static_content_dynamic_size =
                    array_param.static_content_dynamic_size.remove(item);
                let dynamic_content_static_size =
                    array_param.dynamic_content_static_size.remove(item);
                let dynamic_content_dynamic_size =
                    array_param.dynamic_content_dynamic_size.remove(item);
                crate::ParamApplication::Array(crate::ArrayParamApplication {
                    static_content_dynamic_size,
                    dynamic_content_static_size,
                    dynamic_content_dynamic_size,
                })
            }
        }
    }
}

#[allow(clippy::struct_field_names)]
struct ArrayParamApplicationComputeProperties {
    static_content_dynamic_size: ApplicationInstanceComputeProperties,
    dynamic_content_static_size: ApplicationInstanceComputeProperties,
    dynamic_content_dynamic_size: ApplicationInstanceComputeProperties,
}
